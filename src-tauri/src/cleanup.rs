use crate::{
    models::{
        ApprovedPath, CleanHistoryEntry, CleanRequest, CleanResult, CleanupError,
        CleanupErrorCategory, CleanupOutcomeStatus, CleanupPreview, PathCleanupOutcome,
        RejectedPath, SafetyLevel,
    },
    process::{claude_activity, claude_activity_blocks_cleanup, claude_cleanup_block_message},
    quarantine::quarantine_target,
    safety::{
        claude_roots, is_claude_vm_bundle_leaf, is_rebuildable_vm_bundle_artifact, sanitize_path,
        validate_existing_target, validate_existing_target_with_roots, KnownRoot,
        ValidationPurpose,
    },
    scanner::{inspect_tree, io_error, perform_scan},
};
use chrono::Local;
use std::{fs, path::Path, time::Instant};

#[derive(Debug, Default)]
struct RemoveStats {
    files_removed: u64,
    directories_removed: u64,
    errors: Vec<CleanupError>,
    skip_reason: Option<String>,
}

const VM_BUNDLE_IN_PROGRESS_REASON: &str = "VM bundle contains only in-progress .tmp/.partial artifacts. CCW left them untouched. Fully quit Claude/Cowork and retry after the bundle finishes rebuilding.";
const VM_BUNDLE_UNSUPPORTED_REASON: &str =
    "VM bundle contains no approved rebuildable artifacts. CCW left it untouched for safety.";

pub fn preview_cleanup(request: &CleanRequest) -> Result<CleanupPreview, String> {
    preview_cleanup_with_roots(request, &claude_roots())
}

fn preview_cleanup_with_roots(
    request: &CleanRequest,
    roots: &[KnownRoot],
) -> Result<CleanupPreview, String> {
    if request.paths.is_empty() {
        return Err("No cleanup paths were selected.".to_string());
    }
    let activity = claude_activity();
    let cleanup_blocked = claude_activity_blocks_cleanup(activity) && !request.allow_when_running;
    let mut approved_paths = Vec::new();
    let mut rejected_paths = Vec::new();
    let mut approved_canonical = Vec::new();
    let mut warnings = Vec::new();

    for raw in &request.paths {
        let path = Path::new(raw);
        match validate_existing_target_with_roots(
            path,
            roots,
            ValidationPurpose::CleanupAllowCaution,
        ) {
            Ok(validated) => {
                if approved_canonical
                    .iter()
                    .any(|existing: &std::path::PathBuf| {
                        validated.canonical == *existing
                            || validated.canonical.starts_with(existing)
                            || existing.starts_with(&validated.canonical)
                    })
                {
                    rejected_paths.push(RejectedPath {
                        path: raw.clone(),
                        display_path: sanitize_path(path),
                        reason: "Duplicate or overlapping cleanup selections are refused."
                            .to_string(),
                        category: CleanupErrorCategory::InvalidTarget,
                    });
                    continue;
                }
                let mut inspection_errors = Vec::new();
                let stats = inspect_tree(&validated.canonical, &mut inspection_errors);
                if !inspection_errors.is_empty() {
                    warnings.push(format!(
                        "{} item(s) could not be inspected under {}. Cleanup will re-check and report them individually.",
                        inspection_errors.len(),
                        sanitize_path(&validated.canonical)
                    ));
                }
                if let Some(warning) = vm_bundle_skip_warning(&validated.canonical) {
                    warnings.push(warning);
                }
                approved_canonical.push(validated.canonical.clone());
                approved_paths.push(ApprovedPath {
                    path: validated.canonical.to_string_lossy().to_string(),
                    display_path: sanitize_path(&validated.canonical),
                    safety: validated.safety,
                    reason: validated.reason,
                    estimated_bytes: stats.bytes,
                    estimated_file_count: stats.files,
                    estimated_directory_count: stats.directories,
                    requires_quarantine: validated.safety == SafetyLevel::Caution,
                });
            }
            Err(failure) => rejected_paths.push(RejectedPath {
                path: raw.clone(),
                display_path: sanitize_path(path),
                reason: failure.reason,
                category: failure.category,
            }),
        }
    }
    if cleanup_blocked {
        warnings.push(claude_cleanup_block_message(activity));
    }
    if approved_paths.iter().any(|path| path.requires_quarantine) && !request.quarantine_caution {
        warnings.push(
            "Caution targets require the explicit quarantine option before confirmation."
                .to_string(),
        );
    }
    let protected_items_detected = rejected_paths
        .iter()
        .any(|path| path.category == CleanupErrorCategory::ProtectedTarget);
    Ok(CleanupPreview {
        requested_paths: request.paths.clone(),
        estimated_bytes: approved_paths.iter().map(|path| path.estimated_bytes).sum(),
        estimated_file_count: approved_paths
            .iter()
            .map(|path| path.estimated_file_count)
            .sum(),
        estimated_directory_count: approved_paths
            .iter()
            .map(|path| path.estimated_directory_count)
            .sum(),
        approved_paths,
        rejected_paths,
        protected_items_detected,
        claude_activity: activity,
        cleanup_blocked,
        warnings,
        generated_at: Local::now().to_rfc3339(),
    })
}

pub fn perform_cleanup(
    request: &CleanRequest,
    quarantine_root: &Path,
    quarantine_retention_days: i32,
) -> Result<CleanResult, String> {
    if request.paths.is_empty() {
        return Err("No cleanup paths were selected.".to_string());
    }
    let activity = claude_activity();
    if claude_activity_blocks_cleanup(activity) && !request.allow_when_running {
        return Err(claude_cleanup_block_message(activity));
    }
    let started = Instant::now();
    let cleaned_at = Local::now().to_rfc3339();
    let mut outcomes = Vec::new();
    let mut quarantine_used = false;

    for raw in &request.paths {
        let path = Path::new(raw);
        let purpose = if request.quarantine_caution {
            ValidationPurpose::CleanupAllowCaution
        } else {
            ValidationPurpose::CleanupSafeOnly
        };
        let validated = match validate_existing_target(path, purpose) {
            Ok(value) => value,
            Err(failure) => {
                outcomes.push(failed_outcome(path, failure.category, failure.reason));
                continue;
            }
        };

        // This inspection and the policy call above happen immediately before mutation.
        // Preview state is never trusted here.
        let mut inspection_errors = Vec::new();
        let before = inspect_tree(&validated.canonical, &mut inspection_errors);
        if validated.safety == SafetyLevel::Caution {
            match quarantine_target(
                &validated.canonical,
                quarantine_root,
                quarantine_retention_days,
            ) {
                Ok(entry) => {
                    quarantine_used = true;
                    outcomes.push(PathCleanupOutcome {
                        path: validated.canonical.to_string_lossy().to_string(),
                        display_path: sanitize_path(&validated.canonical),
                        status: CleanupOutcomeStatus::Quarantined,
                        estimated_bytes: before.bytes,
                        // Same-volume quarantine moves data out of Claude state but does not free disk bytes.
                        actual_reclaimed_bytes: 0,
                        files_removed: 0,
                        directories_removed: 0,
                        locked_items: Vec::new(),
                        errors: inspection_errors,
                        skip_reason: None,
                        quarantine_cleanup_id: Some(entry.cleanup_id),
                    });
                }
                Err(error) => outcomes.push(PathCleanupOutcome {
                    path: validated.canonical.to_string_lossy().to_string(),
                    display_path: sanitize_path(&validated.canonical),
                    status: CleanupOutcomeStatus::Failed,
                    estimated_bytes: before.bytes,
                    actual_reclaimed_bytes: 0,
                    files_removed: 0,
                    directories_removed: 0,
                    locked_items: locked_paths(std::slice::from_ref(&error)),
                    errors: vec![error],
                    skip_reason: None,
                    quarantine_cleanup_id: None,
                }),
            }
            continue;
        }

        let final_validation = match validate_existing_target(
            &validated.canonical,
            ValidationPurpose::CleanupSafeOnly,
        ) {
            Ok(value) if value.canonical == validated.canonical => value,
            Ok(_) => {
                outcomes.push(failed_outcome(
                    &validated.canonical,
                    CleanupErrorCategory::InvalidTarget,
                    "Cleanup stopped because the canonical target changed after inspection."
                        .to_string(),
                ));
                continue;
            }
            Err(failure) => {
                outcomes.push(failed_outcome(
                    &validated.canonical,
                    failure.category,
                    failure.reason,
                ));
                continue;
            }
        };
        let removal = remove_approved_target(&final_validation.canonical);
        let mut after_errors = Vec::new();
        let after = if validated.canonical.exists() {
            inspect_tree(&validated.canonical, &mut after_errors)
        } else {
            Default::default()
        };
        let actual = before.bytes.saturating_sub(after.bytes);
        let mut errors = inspection_errors;
        errors.extend(removal.errors);
        errors.extend(after_errors);
        let status =
            if actual == 0 && removal.files_removed == 0 && removal.directories_removed == 0 {
                if errors.is_empty() {
                    CleanupOutcomeStatus::Skipped
                } else {
                    CleanupOutcomeStatus::Failed
                }
            } else if after.bytes == 0 && errors.is_empty() {
                CleanupOutcomeStatus::FullyCleaned
            } else {
                CleanupOutcomeStatus::PartiallyCleaned
            };
        outcomes.push(PathCleanupOutcome {
            path: validated.canonical.to_string_lossy().to_string(),
            display_path: sanitize_path(&validated.canonical),
            status,
            estimated_bytes: before.bytes,
            actual_reclaimed_bytes: actual,
            files_removed: removal.files_removed,
            directories_removed: removal.directories_removed,
            locked_items: locked_paths(&errors),
            errors,
            skip_reason: removal.skip_reason,
            quarantine_cleanup_id: None,
        });
    }

    let scan = perform_scan(Vec::new()).ok();
    let errors = outcomes
        .iter()
        .flat_map(|outcome| outcome.errors.clone())
        .collect::<Vec<_>>();
    let locked_items = outcomes
        .iter()
        .flat_map(|outcome| outcome.locked_items.clone())
        .collect::<Vec<_>>();
    let paths_cleaned = outcomes
        .iter()
        .filter(|outcome| {
            matches!(
                outcome.status,
                CleanupOutcomeStatus::FullyCleaned
                    | CleanupOutcomeStatus::PartiallyCleaned
                    | CleanupOutcomeStatus::Quarantined
            )
        })
        .map(|outcome| outcome.display_path.clone())
        .collect();
    let paths_skipped = outcomes
        .iter()
        .filter(|outcome| {
            matches!(
                outcome.status,
                CleanupOutcomeStatus::Skipped | CleanupOutcomeStatus::Failed
            )
        })
        .map(|outcome| outcome.display_path.clone())
        .collect();
    Ok(CleanResult {
        estimated_bytes: outcomes.iter().map(|outcome| outcome.estimated_bytes).sum(),
        actual_reclaimed_bytes: outcomes
            .iter()
            .map(|outcome| outcome.actual_reclaimed_bytes)
            .sum(),
        files_removed: outcomes.iter().map(|outcome| outcome.files_removed).sum(),
        directories_removed: outcomes
            .iter()
            .map(|outcome| outcome.directories_removed)
            .sum(),
        paths_cleaned,
        paths_skipped,
        locked_items,
        errors,
        outcomes,
        duration_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
        trigger: normalize_trigger(&request.trigger),
        quarantine_used,
        remaining_bytes: scan.map(|value| value.total_bytes).unwrap_or(0),
        cleaned_at,
    })
}

pub fn history_from_result(result: &CleanResult) -> CleanHistoryEntry {
    CleanHistoryEntry {
        cleaned_at: result.cleaned_at.clone(),
        estimated_bytes: result.estimated_bytes,
        actual_reclaimed_bytes: result.actual_reclaimed_bytes,
        remaining_bytes: result.remaining_bytes,
        duration_ms: result.duration_ms,
        trigger: result.trigger.clone(),
        quarantine_used: result.quarantine_used,
        outcomes: result
            .outcomes
            .iter()
            .cloned()
            .map(|mut outcome| {
                outcome.path = outcome.display_path.clone();
                outcome
            })
            .collect(),
        errors: result.errors.clone(),
        deleted_paths: result.paths_cleaned.clone(),
        cleaned_bytes: result.actual_reclaimed_bytes,
    }
}

fn failed_outcome(
    path: &Path,
    category: CleanupErrorCategory,
    message: String,
) -> PathCleanupOutcome {
    let display = sanitize_path(path);
    PathCleanupOutcome {
        path: path.to_string_lossy().to_string(),
        display_path: display.clone(),
        status: CleanupOutcomeStatus::Failed,
        estimated_bytes: 0,
        actual_reclaimed_bytes: 0,
        files_removed: 0,
        directories_removed: 0,
        locked_items: Vec::new(),
        errors: vec![CleanupError {
            category,
            path: display,
            message,
        }],
        skip_reason: None,
        quarantine_cleanup_id: None,
    }
}

fn remove_approved_target(path: &Path) -> RemoveStats {
    let mut stats = RemoveStats::default();
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) => {
            stats
                .errors
                .push(io_error(path, &error, CleanupErrorCategory::ReadFailed));
            return stats;
        }
    };
    if metadata.file_type().is_symlink() {
        stats.errors.push(CleanupError {
            category: CleanupErrorCategory::SymlinkRejected,
            path: sanitize_path(path),
            message: "Symbolic link was not followed.".to_string(),
        });
        return stats;
    }
    if metadata.is_file() {
        if is_rebuildable_vm_bundle_artifact(path) {
            remove_file(path, &mut stats);
        }
        return stats;
    }
    if is_claude_vm_bundle_leaf(path) {
        match fs::read_dir(path) {
            Ok(entries) => {
                let mut found_approved_artifact = false;
                let mut found_in_progress_artifact = false;
                for entry in entries.flatten() {
                    let artifact = entry.path();
                    if is_rebuildable_vm_bundle_artifact(&artifact) {
                        found_approved_artifact = true;
                        remove_tree(&artifact, &mut stats);
                    } else if is_in_progress_vm_artifact(&artifact) {
                        found_in_progress_artifact = true;
                    }
                }
                if !found_approved_artifact {
                    stats.skip_reason = Some(
                        if found_in_progress_artifact {
                            VM_BUNDLE_IN_PROGRESS_REASON
                        } else {
                            VM_BUNDLE_UNSUPPORTED_REASON
                        }
                        .to_string(),
                    );
                }
            }
            Err(error) => {
                stats
                    .errors
                    .push(io_error(path, &error, CleanupErrorCategory::ReadFailed))
            }
        }
        return stats;
    }
    match fs::read_dir(path) {
        Ok(entries) => {
            for entry in entries {
                match entry {
                    Ok(entry) => remove_tree(&entry.path(), &mut stats),
                    Err(error) => {
                        stats
                            .errors
                            .push(io_error(path, &error, CleanupErrorCategory::ReadFailed))
                    }
                }
            }
        }
        Err(error) => stats
            .errors
            .push(io_error(path, &error, CleanupErrorCategory::ReadFailed)),
    }
    stats
}

fn vm_bundle_skip_warning(path: &Path) -> Option<String> {
    if !is_claude_vm_bundle_leaf(path) {
        return None;
    }
    let entries = fs::read_dir(path).ok()?;
    if entries
        .flatten()
        .any(|entry| is_in_progress_vm_artifact(&entry.path()))
    {
        Some(VM_BUNDLE_IN_PROGRESS_REASON.to_string())
    } else {
        None
    }
}

fn is_in_progress_vm_artifact(path: &Path) -> bool {
    let Some(name) = path.file_name() else {
        return false;
    };
    let name = name.to_string_lossy().to_ascii_lowercase();
    name.ends_with(".tmp") || name.contains(".partial")
}

fn remove_tree(path: &Path, stats: &mut RemoveStats) {
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) => {
            stats
                .errors
                .push(io_error(path, &error, CleanupErrorCategory::ReadFailed));
            return;
        }
    };
    if metadata.file_type().is_symlink() {
        stats.errors.push(CleanupError {
            category: CleanupErrorCategory::SymlinkRejected,
            path: sanitize_path(path),
            message: "Symbolic link was not followed or removed.".to_string(),
        });
        return;
    }
    if metadata.is_file() {
        remove_file(path, stats);
        return;
    }
    let entries = match fs::read_dir(path) {
        Ok(value) => value,
        Err(error) => {
            stats
                .errors
                .push(io_error(path, &error, CleanupErrorCategory::ReadFailed));
            return;
        }
    };
    for entry in entries {
        match entry {
            Ok(entry) => remove_tree(&entry.path(), stats),
            Err(error) => {
                stats
                    .errors
                    .push(io_error(path, &error, CleanupErrorCategory::ReadFailed))
            }
        }
    }
    match fs::remove_dir(path) {
        Ok(()) => stats.directories_removed += 1,
        Err(error) => stats
            .errors
            .push(io_error(path, &error, CleanupErrorCategory::DeleteFailed)),
    }
}

fn remove_file(path: &Path, stats: &mut RemoveStats) {
    match fs::remove_file(path) {
        Ok(()) => stats.files_removed += 1,
        Err(error) => stats
            .errors
            .push(io_error(path, &error, CleanupErrorCategory::DeleteFailed)),
    }
}

fn locked_paths(errors: &[CleanupError]) -> Vec<String> {
    errors
        .iter()
        .filter(|error| {
            matches!(
                error.category,
                CleanupErrorCategory::FileLocked | CleanupErrorCategory::PermissionDenied
            )
        })
        .map(|error| error.path.clone())
        .collect()
}

fn normalize_trigger(value: &str) -> String {
    match value {
        "manual" | "schedule" | "threshold" | "startup" | "disk_space" | "tray" => {
            value.to_string()
        }
        _ => "manual".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_cleanup_counts_files_and_directories() {
        let root = std::env::temp_dir().join(format!("ccw-remove-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("a").join("b")).unwrap();
        fs::write(root.join("a").join("b").join("cache.bin"), b"cache").unwrap();
        let stats = remove_approved_target(&root);
        assert!(stats.errors.is_empty(), "{:?}", stats.errors);
        assert_eq!(stats.files_removed, 1);
        assert_eq!(stats.directories_removed, 2);
        assert!(root.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn stale_missing_target_is_reported_not_trusted() {
        let root = std::env::temp_dir().join(format!("ccw-stale-{}", std::process::id()));
        let failure =
            validate_existing_target(&root, ValidationPurpose::CleanupSafeOnly).unwrap_err();
        assert_eq!(failure.category, CleanupErrorCategory::PathNotFound);
    }

    #[test]
    fn stale_preview_does_not_authorize_a_removed_target() {
        let root = std::env::temp_dir().join(format!("ccw-preview-stale-{}", std::process::id()));
        let cache = root.join("Cache");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&cache).unwrap();
        fs::write(cache.join("entry.bin"), b"cache").unwrap();
        let roots = vec![KnownRoot {
            path: root.clone(),
            label: "test".to_string(),
            safety: SafetyLevel::NotRecommended,
            default_cleanup: false,
            description: "protected test root".to_string(),
        }];
        let request = CleanRequest {
            paths: vec![cache.to_string_lossy().to_string()],
            allow_when_running: true,
            quarantine_caution: false,
            trigger: "manual".to_string(),
        };
        let preview = preview_cleanup_with_roots(&request, &roots).unwrap();
        assert_eq!(preview.approved_paths.len(), 1);
        fs::remove_dir_all(&cache).unwrap();
        let failure =
            validate_existing_target_with_roots(&cache, &roots, ValidationPurpose::CleanupSafeOnly)
                .unwrap_err();
        assert_eq!(failure.category, CleanupErrorCategory::PathNotFound);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn partial_deletion_keeps_symlink_and_reports_it() {
        let root = std::env::temp_dir().join(format!("ccw-partial-{}", std::process::id()));
        let outside =
            std::env::temp_dir().join(format!("ccw-partial-outside-{}", std::process::id()));
        let link = root.join("linked");
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(root.join("normal.bin"), b"cache").unwrap();
        if create_test_dir_symlink(&outside, &link) {
            let stats = remove_approved_target(&root);
            assert_eq!(stats.files_removed, 1);
            assert!(stats
                .errors
                .iter()
                .any(|error| error.category == CleanupErrorCategory::SymlinkRejected));
            assert!(link.exists());
        }
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }

    #[test]
    fn in_progress_vm_bundle_is_left_untouched_with_a_reason() {
        let root = std::env::temp_dir().join(format!("ccw-vm-progress-{}", std::process::id()));
        let bundle = root.join("vm_bundles").join("claudevm.bundle");
        let pending = bundle.join("rootfs.vhdx.tmp");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&bundle).unwrap();
        fs::write(&pending, b"still building").unwrap();

        let stats = remove_approved_target(&bundle);

        assert_eq!(stats.files_removed, 0);
        assert!(pending.exists());
        assert_eq!(
            stats.skip_reason.as_deref(),
            Some(VM_BUNDLE_IN_PROGRESS_REASON)
        );
        assert_eq!(
            vm_bundle_skip_warning(&bundle).as_deref(),
            Some(VM_BUNDLE_IN_PROGRESS_REASON)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "windows")]
    fn create_test_dir_symlink(target: &Path, link: &Path) -> bool {
        std::os::windows::fs::symlink_dir(target, link).is_ok()
    }

    #[cfg(not(target_os = "windows"))]
    fn create_test_dir_symlink(target: &Path, link: &Path) -> bool {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
}
