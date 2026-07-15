use crate::{
    models::{
        CleanupError, CleanupErrorCategory, QuarantineActionResult, QuarantineEntry,
        QuarantineStatus, SafetyLevel,
    },
    process::{claude_activity, claude_activity_blocks_cleanup},
    safety::{
        claude_roots, sanitize_path, validate_existing_target_with_roots,
        validate_restore_destination_with_roots, KnownRoot, ValidationPurpose,
    },
    scanner::{inspect_tree, io_error},
    state::atomic_write,
};
use chrono::{Duration, Local};
use std::{fs, path::Path};

pub fn quarantine_target(
    target: &Path,
    quarantine_root: &Path,
    retention_days: i32,
) -> Result<QuarantineEntry, CleanupError> {
    quarantine_target_with_roots(target, quarantine_root, retention_days, &claude_roots())
}

fn quarantine_target_with_roots(
    target: &Path,
    quarantine_root: &Path,
    retention_days: i32,
    roots: &[KnownRoot],
) -> Result<QuarantineEntry, CleanupError> {
    let validated =
        validate_existing_target_with_roots(target, roots, ValidationPurpose::CleanupAllowCaution)
            .map_err(|failure| CleanupError {
                category: failure.category,
                path: sanitize_path(target),
                message: failure.reason,
            })?;
    if validated.safety != SafetyLevel::Caution {
        return Err(CleanupError {
            category: CleanupErrorCategory::InvalidTarget,
            path: sanitize_path(target),
            message: "Quarantine is reserved for Caution targets; Safe cache is cleaned directly and Protected data is refused.".to_string(),
        });
    }
    fs::create_dir_all(quarantine_root)
        .map_err(|error| io_error(quarantine_root, &error, CleanupErrorCategory::MoveFailed))?;
    if !same_volume(&validated.canonical, quarantine_root) {
        return Err(CleanupError {
            category: CleanupErrorCategory::MoveFailed,
            path: sanitize_path(&validated.canonical),
            message: "Quarantine requires a same-volume atomic move; copy fallback is intentionally disabled.".to_string(),
        });
    }
    let cleanup_id = format!(
        "{}-{}",
        Local::now().format("%Y%m%d%H%M%S%3f"),
        std::process::id()
    );
    let entry_dir = quarantine_root.join(&cleanup_id);
    let payload = entry_dir.join("payload");
    fs::create_dir_all(&entry_dir)
        .map_err(|error| io_error(&entry_dir, &error, CleanupErrorCategory::MoveFailed))?;
    let mut scan_errors = Vec::new();
    let stats = inspect_tree(&validated.canonical, &mut scan_errors);
    if !scan_errors.is_empty() {
        let _ = fs::remove_dir_all(&entry_dir);
        return Err(CleanupError {
            category: CleanupErrorCategory::ReadFailed,
            path: sanitize_path(&validated.canonical),
            message: "Quarantine was cancelled because the complete target could not be inspected safely.".to_string(),
        });
    }
    let final_validation = match validate_existing_target_with_roots(
        &validated.canonical,
        roots,
        ValidationPurpose::CleanupAllowCaution,
    ) {
        Ok(value) => value,
        Err(failure) => {
            let _ = fs::remove_dir_all(&entry_dir);
            return Err(CleanupError {
                category: failure.category,
                path: sanitize_path(&validated.canonical),
                message: failure.reason,
            });
        }
    };
    if final_validation.canonical != validated.canonical {
        let _ = fs::remove_dir_all(&entry_dir);
        return Err(CleanupError {
            category: CleanupErrorCategory::InvalidTarget,
            path: sanitize_path(&validated.canonical),
            message:
                "Quarantine stopped because the canonical target changed before the atomic move."
                    .to_string(),
        });
    }
    fs::rename(&validated.canonical, &payload).map_err(|error| {
        let _ = fs::remove_dir_all(&entry_dir);
        io_error(
            &validated.canonical,
            &error,
            CleanupErrorCategory::MoveFailed,
        )
    })?;
    let created_at = Local::now();
    let expiry_date = if retention_days < 0 {
        None
    } else {
        Some((created_at + Duration::days(i64::from(retention_days.max(1)))).to_rfc3339())
    };
    let entry = QuarantineEntry {
        cleanup_id,
        created_at: created_at.to_rfc3339(),
        original_path: validated.canonical.to_string_lossy().to_string(),
        display_original_path: sanitize_path(&validated.canonical),
        quarantine_path: payload.to_string_lossy().to_string(),
        size_bytes: stats.bytes,
        file_count: stats.files,
        status: QuarantineStatus::Quarantined,
        restore_eligible: true,
        expiry_date,
        errors: Vec::new(),
    };
    if let Err(message) = write_manifest(&entry_dir, &entry) {
        // The manifest is required for safe restoration. Roll the atomic move back.
        let rollback = fs::rename(&payload, &validated.canonical);
        let _ = fs::remove_dir_all(&entry_dir);
        return Err(CleanupError {
            category: CleanupErrorCategory::MoveFailed,
            path: sanitize_path(&validated.canonical),
            message: if rollback.is_ok() {
                format!("Quarantine manifest could not be persisted; the move was rolled back: {message}")
            } else {
                format!("Quarantine manifest failed and rollback also failed. Manual recovery is required from {}: {message}", payload.display())
            },
        });
    }
    Ok(entry)
}

pub fn list_quarantine_entries(quarantine_root: &Path) -> Result<Vec<QuarantineEntry>, String> {
    if !quarantine_root.exists() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for item in fs::read_dir(quarantine_root).map_err(|error| error.to_string())? {
        let item = match item {
            Ok(value) => value,
            Err(_) => continue,
        };
        if !item.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        if let Ok(entry) = read_manifest(&item.path()) {
            entries.push(entry);
        }
    }
    entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(entries)
}

pub fn restore_quarantine_entry(
    quarantine_root: &Path,
    cleanup_id: &str,
) -> Result<QuarantineActionResult, String> {
    restore_quarantine_entry_with_roots(
        quarantine_root,
        cleanup_id,
        &claude_roots(),
        claude_activity(),
    )
}

fn restore_quarantine_entry_with_roots(
    quarantine_root: &Path,
    cleanup_id: &str,
    roots: &[KnownRoot],
    activity: crate::models::ClaudeActivity,
) -> Result<QuarantineActionResult, String> {
    reject_invalid_cleanup_id(cleanup_id)?;
    let entry_dir = quarantine_root.join(cleanup_id);
    let mut entry = read_manifest(&entry_dir)?;
    if claude_activity_blocks_cleanup(activity) {
        return Err("Claude must be fully closed before restoring quarantined data.".to_string());
    }
    if entry.status != QuarantineStatus::Quarantined || !entry.restore_eligible {
        return Err("This quarantine entry is not eligible for restoration.".to_string());
    }
    let original = Path::new(&entry.original_path);
    validate_restore_destination_with_roots(original, roots).map_err(|failure| failure.reason)?;
    let payload = Path::new(&entry.quarantine_path);
    let metadata = fs::symlink_metadata(payload).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() {
        return Err("Quarantine payload is a symbolic link and cannot be restored.".to_string());
    }
    fs::rename(payload, original)
        .map_err(|error| format!("Restore failed without modifying the destination: {error}"))?;
    entry.status = QuarantineStatus::Restored;
    entry.restore_eligible = false;
    write_manifest(&entry_dir, &entry)?;
    Ok(QuarantineActionResult {
        cleanup_id: cleanup_id.to_string(),
        status: QuarantineStatus::Restored,
        errors: Vec::new(),
    })
}

pub fn permanently_delete_quarantine_entry(
    quarantine_root: &Path,
    cleanup_id: &str,
) -> Result<QuarantineActionResult, String> {
    reject_invalid_cleanup_id(cleanup_id)?;
    let entry_dir = quarantine_root.join(cleanup_id);
    let entry = read_manifest(&entry_dir)?;
    let payload = Path::new(&entry.quarantine_path);
    if payload.exists() {
        delete_tree_without_following_links(payload).map_err(|error| error.message)?;
    }
    fs::remove_dir_all(&entry_dir).map_err(|error| error.to_string())?;
    Ok(QuarantineActionResult {
        cleanup_id: cleanup_id.to_string(),
        status: QuarantineStatus::Deleted,
        errors: Vec::new(),
    })
}

pub fn clear_expired_quarantine(
    quarantine_root: &Path,
) -> Result<Vec<QuarantineActionResult>, String> {
    if claude_activity_blocks_cleanup(claude_activity()) {
        return Err(
            "Expired Caution quarantine entries are not cleared while Claude is active."
                .to_string(),
        );
    }
    let now = Local::now();
    let entries = list_quarantine_entries(quarantine_root)?;
    let mut results = Vec::new();
    for entry in entries {
        let expired = entry
            .expiry_date
            .as_deref()
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.with_timezone(&Local) <= now)
            .unwrap_or(false);
        if expired && entry.status == QuarantineStatus::Quarantined {
            results.push(permanently_delete_quarantine_entry(
                quarantine_root,
                &entry.cleanup_id,
            )?);
        }
    }
    Ok(results)
}

fn write_manifest(entry_dir: &Path, entry: &QuarantineEntry) -> Result<(), String> {
    let value = serde_json::to_vec_pretty(entry).map_err(|error| error.to_string())?;
    atomic_write(&entry_dir.join("manifest.json"), &value)
}

fn read_manifest(entry_dir: &Path) -> Result<QuarantineEntry, String> {
    let value =
        fs::read_to_string(entry_dir.join("manifest.json")).map_err(|error| error.to_string())?;
    serde_json::from_str(&value).map_err(|error| error.to_string())
}

fn reject_invalid_cleanup_id(cleanup_id: &str) -> Result<(), String> {
    if cleanup_id.is_empty()
        || !cleanup_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        Err("Invalid quarantine cleanup id.".to_string())
    } else {
        Ok(())
    }
}

fn delete_tree_without_following_links(path: &Path) -> Result<(), CleanupError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| io_error(path, &error, CleanupErrorCategory::DeleteFailed))?;
    if metadata.file_type().is_symlink() {
        return Err(CleanupError {
            category: CleanupErrorCategory::SymlinkRejected,
            path: sanitize_path(path),
            message: "Quarantine deletion stopped at a symbolic link.".to_string(),
        });
    }
    if metadata.is_file() {
        return fs::remove_file(path)
            .map_err(|error| io_error(path, &error, CleanupErrorCategory::DeleteFailed));
    }
    for entry in fs::read_dir(path)
        .map_err(|error| io_error(path, &error, CleanupErrorCategory::ReadFailed))?
    {
        let entry =
            entry.map_err(|error| io_error(path, &error, CleanupErrorCategory::ReadFailed))?;
        delete_tree_without_following_links(&entry.path())?;
    }
    fs::remove_dir(path).map_err(|error| io_error(path, &error, CleanupErrorCategory::DeleteFailed))
}

#[cfg(target_os = "windows")]
fn same_volume(source: &Path, quarantine_root: &Path) -> bool {
    use std::path::{Component, Prefix};
    fn drive(path: &Path) -> Option<u8> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        canonical
            .components()
            .find_map(|component| match component {
                Component::Prefix(value) => match value.kind() {
                    Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
                        Some(letter.to_ascii_uppercase())
                    }
                    _ => None,
                },
                _ => None,
            })
    }
    let source_drive = drive(source);
    source_drive.is_some() && source_drive == drive(quarantine_root)
}

#[cfg(not(target_os = "windows"))]
fn same_volume(source: &Path, quarantine_root: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let source_dev = fs::metadata(source).map(|value| value.dev()).ok();
    let target_dev = fs::metadata(quarantine_root).map(|value| value.dev()).ok();
    source_dev.is_some() && source_dev == target_dev
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarantine_ids_cannot_traverse() {
        assert!(reject_invalid_cleanup_id("../state").is_err());
        assert!(reject_invalid_cleanup_id("20260715-42").is_ok());
    }

    #[test]
    fn restore_refuses_existing_destination() {
        let root =
            std::env::temp_dir().join(format!("ccw-restore-conflict-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let failure = validate_restore_destination_with_roots(&root, &[]).unwrap_err();
        assert_eq!(failure.category, CleanupErrorCategory::RestoreConflict);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn caution_target_is_atomically_quarantined_and_listed() {
        let root = std::env::temp_dir().join(format!("ccw-quarantine-{}", std::process::id()));
        let claude_root = root.join("ClaudeRoot");
        let caution = claude_root.join("unknown-rebuildable-area");
        let quarantine = root.join("app-data").join("quarantine");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&caution).unwrap();
        fs::create_dir_all(&quarantine).unwrap();
        fs::write(caution.join("cache.bin"), b"cache").unwrap();
        let roots = vec![KnownRoot {
            path: claude_root,
            label: "test".to_string(),
            safety: SafetyLevel::NotRecommended,
            default_cleanup: false,
            description: "protected root".to_string(),
        }];

        let entry = quarantine_target_with_roots(&caution, &quarantine, 7, &roots).unwrap();
        assert!(!caution.exists());
        assert!(Path::new(&entry.quarantine_path).exists());
        let listed = list_quarantine_entries(&quarantine).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].cleanup_id, entry.cleanup_id);
        let restored = restore_quarantine_entry_with_roots(
            &quarantine,
            &entry.cleanup_id,
            &roots,
            crate::models::ClaudeActivity::NotDetected,
        )
        .unwrap();
        assert_eq!(restored.status, QuarantineStatus::Restored);
        assert!(caution.exists());
        assert_eq!(fs::read(caution.join("cache.bin")).unwrap(), b"cache");
        permanently_delete_quarantine_entry(&quarantine, &entry.cleanup_id).unwrap();
        assert!(list_quarantine_entries(&quarantine).unwrap().is_empty());
        let _ = fs::remove_dir_all(root);
    }
}
