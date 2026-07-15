use crate::models::{CleanHistoryEntry, PersistedState, SchedulerSettings};
use chrono::{DateTime, Local};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Mutex},
    time::SystemTime,
};
use tauri::Manager;

pub const MAX_SAMPLES: usize = 96;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootSignature {
    pub path: PathBuf,
    pub modified: Option<SystemTime>,
}

#[derive(Debug, Clone)]
pub struct SchedulerScanCache {
    pub scanned_at: DateTime<Local>,
    pub root_signature: Vec<RootSignature>,
}

pub struct AppState {
    pub persisted: Mutex<PersistedState>,
    save_lock: Mutex<()>,
    pub scheduler_scan_cache: Mutex<Option<SchedulerScanCache>>,
    pub startup_evaluated: AtomicBool,
    pub storage_path: PathBuf,
    pub quarantine_root: PathBuf,
    load_warnings: Mutex<Vec<String>>,
}

impl AppState {
    pub fn load(app: tauri::AppHandle) -> Self {
        let config_dir = app.path().app_config_dir().unwrap_or_else(|_| {
            dirs::config_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("ClaudeCacheWarden")
        });
        let data_dir = app
            .path()
            .app_data_dir()
            .unwrap_or_else(|_| config_dir.clone());
        let _ = fs::create_dir_all(&config_dir);
        let _ = fs::create_dir_all(&data_dir);
        let storage_path = config_dir.join("state.json");
        let quarantine_root = data_dir.join("quarantine");
        let _ = fs::create_dir_all(&quarantine_root);
        let (persisted, warnings) = load_persisted_state(&storage_path);
        Self {
            persisted: Mutex::new(persisted),
            save_lock: Mutex::new(()),
            scheduler_scan_cache: Mutex::new(None),
            startup_evaluated: AtomicBool::new(false),
            storage_path,
            quarantine_root,
            load_warnings: Mutex::new(warnings),
        }
    }

    pub fn warnings(&self) -> Vec<String> {
        self.load_warnings
            .lock()
            .map(|value| value.clone())
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub fn for_test(storage_path: PathBuf, quarantine_root: PathBuf) -> Self {
        let (persisted, warnings) = load_persisted_state(&storage_path);
        Self {
            persisted: Mutex::new(persisted),
            save_lock: Mutex::new(()),
            scheduler_scan_cache: Mutex::new(None),
            startup_evaluated: AtomicBool::new(false),
            storage_path,
            quarantine_root,
            load_warnings: Mutex::new(warnings),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let _save_guard = self
            .save_lock
            .lock()
            .map_err(|_| "State save lock failed".to_string())?;
        let value = {
            let state = self
                .persisted
                .lock()
                .map_err(|_| "State lock failed".to_string())?;
            serde_json::to_vec_pretty(&*state).map_err(|error| error.to_string())?
        };
        atomic_write(&self.storage_path, &value)
    }

    pub fn settings(&self) -> Result<SchedulerSettings, String> {
        self.persisted
            .lock()
            .map(|value| value.settings.clone())
            .map_err(|_| "State lock failed".to_string())
    }

    pub fn record_sample(&self, total_bytes: u64) -> Result<(), String> {
        {
            let mut state = self
                .persisted
                .lock()
                .map_err(|_| "State lock failed".to_string())?;
            state.samples.push_back(crate::models::SizeSample {
                captured_at: Local::now(),
                total_bytes,
            });
            while state.samples.len() > MAX_SAMPLES {
                state.samples.pop_front();
            }
        }
        self.save()
    }

    pub fn push_history(&self, entry: CleanHistoryEntry) -> Result<(), String> {
        {
            let mut state = self
                .persisted
                .lock()
                .map_err(|_| "State lock failed".to_string())?;
            state.last_cleanup_at = DateTime::parse_from_rfc3339(&entry.cleaned_at)
                .ok()
                .map(|value| value.with_timezone(&Local));
            state.history.insert(0, entry);
            state.history.truncate(100);
        }
        self.save()
    }
}

fn load_persisted_state(path: &Path) -> (PersistedState, Vec<String>) {
    let backup = backup_path(path);
    match read_json(path) {
        Ok(Some(state)) => (state, Vec::new()),
        Ok(None) => (PersistedState::default(), Vec::new()),
        Err(main_error) => match read_json(&backup) {
            Ok(Some(state)) => (
                state,
                vec![format!("State file was corrupted; settings were recovered from backup. Main error: {main_error}")],
            ),
            Ok(None) => (
                PersistedState::default(),
                vec![format!("State file was corrupted and no backup exists. Defaults are active until settings are saved. Main error: {main_error}")],
            ),
            Err(backup_error) => (
                PersistedState::default(),
                vec![format!("State and backup could not be parsed. Defaults are active until settings are saved. Main error: {main_error}; backup error: {backup_error}")],
            ),
        },
    }
}

fn read_json(path: &Path) -> Result<Option<PersistedState>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let mut value = String::new();
    File::open(path)
        .and_then(|mut file| file.read_to_string(&mut value))
        .map_err(|error| error.to_string())?;
    serde_json::from_str(&value)
        .map(Some)
        .map_err(|error| error.to_string())
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "State path has no parent".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    {
        let mut file = File::create(&temporary).map_err(|error| error.to_string())?;
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
    }
    if path.exists() {
        fs::copy(path, backup_path(path)).map_err(|error| error.to_string())?;
    }
    atomic_replace(&temporary, path).inspect_err(|_| {
        let _ = fs::remove_file(&temporary);
    })?;
    Ok(())
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

#[cfg(target_os = "windows")]
fn atomic_replace(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let source_wide = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination_wide = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(())
    }
}

#[cfg(not(target_os = "windows"))]
fn atomic_replace(source: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(source, destination).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_state_recovers_from_backup() {
        let root = std::env::temp_dir().join(format!("ccw-state-{}", std::process::id()));
        let state_path = root.join("state.json");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let mut initial = PersistedState::default();
        initial.settings.minimum_free_gb = 22.0;
        atomic_write(&state_path, &serde_json::to_vec(&initial).unwrap()).unwrap();
        let mut updated = initial.clone();
        updated.settings.minimum_free_gb = 33.0;
        atomic_write(&state_path, &serde_json::to_vec(&updated).unwrap()).unwrap();
        fs::write(&state_path, b"not-json").unwrap();

        let (recovered, warnings) = load_persisted_state(&state_path);
        assert_eq!(recovered.settings.minimum_free_gb, 22.0);
        assert!(!warnings.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_history_errors_migrate_without_resetting_settings() {
        let root = std::env::temp_dir().join(format!("ccw-state-legacy-{}", std::process::id()));
        let state_path = root.join("state.json");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let legacy = serde_json::json!({
            "settings": {
                "enabled": true,
                "schedule_enabled": true,
                "schedule_time": "03:00",
                "threshold_enabled": true,
                "threshold_gb": 9.0,
                "growth_alert_enabled": true,
                "growth_alert_gb_per_hour": 2.0,
                "clean_when_claude_running": false
            },
            "history": [{
                "cleaned_at": "2026-07-15T00:00:00+07:00",
                "cleaned_bytes": 4,
                "remaining_bytes": 8,
                "trigger": "manual",
                "deleted_paths": ["C:\\Users\\Test\\Cache"],
                "errors": ["legacy locked file"]
            }],
            "samples": [],
            "last_schedule_day": "2026-07-15"
        });
        fs::write(&state_path, serde_json::to_vec(&legacy).unwrap()).unwrap();
        let (loaded, warnings) = load_persisted_state(&state_path);
        assert!(warnings.is_empty());
        assert!(loaded.settings.enabled);
        assert_eq!(loaded.settings.schedule_time, "03:00");
        assert_eq!(loaded.history[0].errors[0].message, "legacy locked file");
        let _ = fs::remove_dir_all(root);
    }
}
