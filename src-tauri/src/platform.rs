use crate::{
    models::VolumeStatus,
    safety::{sanitize_path, validate_existing_target, ValidationPurpose},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use sysinfo::Disks;
use tauri::{AppHandle, Manager};

pub fn report_directory(app: &AppHandle) -> PathBuf {
    app.path()
        .document_dir()
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir()))
}

pub fn reveal_path(path: &str, quarantine_root: &Path) -> Result<(), String> {
    let requested = Path::new(path);
    let canonical = if is_approved_quarantine_path(requested, quarantine_root) {
        requested
            .canonicalize()
            .map_err(|error| error.to_string())?
    } else {
        validate_existing_target(requested, ValidationPurpose::Inspect)
            .map_err(|failure| failure.reason)?
            .canonical
    };
    let status = match std::env::consts::OS {
        "windows" => Command::new("explorer.exe")
            .arg("/select,")
            .arg(&canonical)
            .status(),
        "macos" => Command::new("open").arg("-R").arg(&canonical).status(),
        _ => {
            return Err(
                "Reveal in file manager is supported on Windows and macOS only.".to_string(),
            )
        }
    }
    .map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("File manager exited with status {status}"))
    }
}

pub fn reveal_exported_report(app: &AppHandle, path: &str) -> Result<(), String> {
    let report_directory = report_directory(app);
    if !is_approved_report_path(Path::new(path), &report_directory) {
        return Err(
            "Only report files created by Claude Cache Warden can be opened here.".to_string(),
        );
    }
    open_folder(&report_directory)
}

fn open_folder(path: &Path) -> Result<(), String> {
    let status = match std::env::consts::OS {
        "windows" => Command::new("explorer.exe").arg(path).status(),
        "macos" => Command::new("open").arg(path).status(),
        _ => return Err("Open folder is supported on Windows and macOS only.".to_string()),
    }
    .map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("File manager exited with status {status}"))
    }
}

fn is_approved_report_path(path: &Path, report_directory: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return false;
    }
    let Ok(candidate) = path.canonicalize() else {
        return false;
    };
    let Ok(root) = report_directory.canonicalize() else {
        return false;
    };
    let valid_name = candidate
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("claude-cache-report-") && name.ends_with(".json"));
    valid_name && candidate.parent() == Some(root.as_path())
}

fn is_approved_quarantine_path(path: &Path, quarantine_root: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if metadata.file_type().is_symlink() {
        return false;
    }
    let Ok(candidate) = path.canonicalize() else {
        return false;
    };
    let Ok(root) = quarantine_root.canonicalize() else {
        return false;
    };
    candidate == root || candidate.starts_with(root)
}

pub fn configure_launch_at_login(enabled: bool) -> Result<(), String> {
    match std::env::consts::OS {
        "windows" => configure_windows_launch_at_login(enabled),
        "macos" if enabled => Err("Launch at login is not enabled on macOS until a signed Login Item implementation is available.".to_string()),
        "macos" => Ok(()),
        _ if enabled => Err("Launch at login is unsupported on this platform.".to_string()),
        _ => Ok(()),
    }
}

#[cfg(target_os = "windows")]
fn configure_windows_launch_at_login(enabled: bool) -> Result<(), String> {
    let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    let status = if enabled {
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        let value = format!("\"{}\" --minimized", executable.display());
        Command::new("reg.exe")
            .args(["add", key, "/v", "ClaudeCacheWarden", "/t", "REG_SZ", "/d"])
            .arg(value)
            .arg("/f")
            .status()
    } else {
        Command::new("reg.exe")
            .args(["delete", key, "/v", "ClaudeCacheWarden", "/f"])
            .status()
    }
    .map_err(|error| error.to_string())?;
    // Deleting a missing value returns a failure status but the desired disabled state is already true.
    if status.success() || !enabled {
        Ok(())
    } else {
        Err(format!(
            "Windows startup registration failed with status {status}"
        ))
    }
}

#[cfg(not(target_os = "windows"))]
fn configure_windows_launch_at_login(_enabled: bool) -> Result<(), String> {
    Err("Windows launch-at-login registration is unavailable on this platform.".to_string())
}

pub fn volume_status(requested_volume: &str) -> Result<VolumeStatus, String> {
    let disks = Disks::new_with_refreshed_list();
    let requested = if requested_volume.trim().is_empty() {
        default_volume_path()
    } else {
        PathBuf::from(requested_volume)
    };
    let canonical_or_requested = requested.canonicalize().unwrap_or(requested);
    let mut matches = disks
        .iter()
        .filter(|disk| {
            canonical_or_requested.starts_with(disk.mount_point())
                || canonical_or_requested == disk.mount_point()
        })
        .collect::<Vec<_>>();
    matches.sort_by_key(|disk| std::cmp::Reverse(disk.mount_point().components().count()));
    let disk = matches.first().ok_or_else(|| {
        format!(
            "Volume is unavailable: {}",
            sanitize_path(&canonical_or_requested)
        )
    })?;
    let total = disk.total_space();
    let available = disk.available_space();
    Ok(VolumeStatus {
        volume: disk.mount_point().to_string_lossy().to_string(),
        available_bytes: available,
        total_bytes: total,
        free_percentage: if total == 0 {
            0.0
        } else {
            available as f64 * 100.0 / total as f64
        },
    })
}

fn default_volume_path() -> PathBuf {
    crate::safety::claude_roots()
        .into_iter()
        .find(|root| root.path.exists())
        .map(|root| root.path)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir()))
}

pub fn launched_minimized() -> bool {
    std::env::args().any(|argument| argument == "--minimized")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarantine_validation_rejects_missing_and_outside_paths() {
        let root = std::env::temp_dir().join(format!("ccw-platform-{}", std::process::id()));
        let outside =
            std::env::temp_dir().join(format!("ccw-platform-outside-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
        fs::create_dir_all(root.join("entry")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        assert!(is_approved_quarantine_path(&root.join("entry"), &root));
        assert!(!is_approved_quarantine_path(&outside, &root));
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }

    #[test]
    fn report_location_validation_accepts_only_direct_ccw_reports() {
        let root = std::env::temp_dir().join(format!("ccw-reports-{}", std::process::id()));
        let nested = root.join("nested");
        let report = root.join("claude-cache-report-20260715-120000.json");
        let unrelated = root.join("notes.json");
        let nested_report = nested.join("claude-cache-report-20260715-120000.json");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&nested).unwrap();
        fs::write(&report, "{}").unwrap();
        fs::write(&unrelated, "{}").unwrap();
        fs::write(&nested_report, "{}").unwrap();
        assert!(is_approved_report_path(&report, &root));
        assert!(!is_approved_report_path(&unrelated, &root));
        assert!(!is_approved_report_path(&nested_report, &root));
        let _ = fs::remove_dir_all(root);
    }
}
