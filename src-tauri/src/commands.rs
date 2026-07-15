use crate::{
    cleanup::{history_from_result, perform_cleanup, preview_cleanup as build_preview},
    models::{
        ClaudeActivity, CleanHistoryEntry, CleanRequest, CleanResult, CleanupPreview,
        FileTypeBreakdownResult, GrowthAlert, LargestItemsResult, QuarantineActionResult,
        QuarantineEntry, SchedulerSettings, VolumeStatus,
    },
    platform::{
        configure_launch_at_login, report_directory, reveal_exported_report, reveal_path,
        volume_status,
    },
    process::claude_activity,
    quarantine::{
        clear_expired_quarantine as clear_expired, list_quarantine_entries as list_entries,
        permanently_delete_quarantine_entry as delete_entry,
        restore_quarantine_entry as restore_entry,
    },
    safety::sanitize_path,
    scanner::{
        get_file_type_breakdown as build_breakdown, get_largest_items as build_largest,
        perform_scan,
    },
    scheduler::{calculate_growth_alert, normalize_settings},
    state::AppState,
};
use chrono::Local;
use std::{fs, path::Path, sync::Arc};
use tauri::State;

#[tauri::command]
pub fn scan_cache(state: State<'_, Arc<AppState>>) -> Result<crate::models::ScanResult, String> {
    let result = perform_scan(state.warnings())?;
    let _ = state.record_sample(result.total_bytes);
    Ok(result)
}

#[tauri::command]
pub fn preview_cleanup(request: CleanRequest) -> Result<CleanupPreview, String> {
    build_preview(&request)
}

#[tauri::command]
pub fn clean_cache(
    state: State<'_, Arc<AppState>>,
    request: CleanRequest,
) -> Result<CleanResult, String> {
    let settings = state.settings()?;
    let result = perform_cleanup(
        &request,
        &state.quarantine_root,
        settings.quarantine_retention_days,
    )?;
    state.push_history(history_from_result(&result))?;
    Ok(result)
}

#[tauri::command]
pub fn get_scheduler_settings(
    state: State<'_, Arc<AppState>>,
) -> Result<SchedulerSettings, String> {
    state.settings()
}

#[tauri::command]
pub fn save_scheduler_settings(
    state: State<'_, Arc<AppState>>,
    settings: SchedulerSettings,
) -> Result<SchedulerSettings, String> {
    let normalized = normalize_settings(settings);
    let previous = state.settings()?;
    if normalized.launch_at_login != previous.launch_at_login {
        configure_launch_at_login(normalized.launch_at_login)?;
    }
    {
        let mut persisted = state
            .persisted
            .lock()
            .map_err(|_| "State lock failed".to_string())?;
        persisted.settings = normalized.clone();
    }
    if let Err(error) = state.save() {
        if let Ok(mut persisted) = state.persisted.lock() {
            persisted.settings = previous.clone();
        }
        if normalized.launch_at_login != previous.launch_at_login {
            let _ = configure_launch_at_login(previous.launch_at_login);
        }
        return Err(error);
    }
    Ok(normalized)
}

#[tauri::command]
pub fn get_clean_history(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<CleanHistoryEntry>, String> {
    state
        .persisted
        .lock()
        .map(|value| value.history.clone())
        .map_err(|_| "State lock failed".to_string())
}

#[tauri::command]
pub fn evaluate_growth_alert(state: State<'_, Arc<AppState>>) -> Result<GrowthAlert, String> {
    let persisted = state
        .persisted
        .lock()
        .map_err(|_| "State lock failed".to_string())?;
    Ok(calculate_growth_alert(&persisted))
}

#[tauri::command]
pub fn get_claude_running() -> bool {
    claude_activity() != ClaudeActivity::NotDetected
}

#[tauri::command]
pub fn get_claude_activity() -> ClaudeActivity {
    claude_activity()
}

#[tauri::command]
pub fn list_quarantine_entries(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<QuarantineEntry>, String> {
    list_entries(&state.quarantine_root)
}

#[tauri::command]
pub fn restore_quarantine_entry(
    state: State<'_, Arc<AppState>>,
    cleanup_id: String,
) -> Result<QuarantineActionResult, String> {
    restore_entry(&state.quarantine_root, &cleanup_id)
}

#[tauri::command]
pub fn permanently_delete_quarantine_entry(
    state: State<'_, Arc<AppState>>,
    cleanup_id: String,
) -> Result<QuarantineActionResult, String> {
    delete_entry(&state.quarantine_root, &cleanup_id)
}

#[tauri::command]
pub fn clear_expired_quarantine(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<QuarantineActionResult>, String> {
    clear_expired(&state.quarantine_root)
}

#[tauri::command]
pub fn open_in_file_manager(state: State<'_, Arc<AppState>>, path: String) -> Result<(), String> {
    reveal_path(&path, &state.quarantine_root)
}

#[tauri::command]
pub fn open_export_location(app: tauri::AppHandle, path: String) -> Result<(), String> {
    reveal_exported_report(&app, &path)
}

#[tauri::command]
pub fn get_largest_items(root: String, limit: usize) -> Result<LargestItemsResult, String> {
    build_largest(&root, limit)
}

#[tauri::command]
pub fn get_file_type_breakdown(root: String) -> Result<FileTypeBreakdownResult, String> {
    build_breakdown(&root)
}

#[tauri::command]
pub fn get_volume_status(volume: String) -> Result<VolumeStatus, String> {
    volume_status(&volume)
}

#[tauri::command]
pub fn export_report(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    unsanitized: bool,
) -> Result<String, String> {
    let mut scan = perform_scan(state.warnings())?;
    let mut persisted = state
        .persisted
        .lock()
        .map_err(|_| "State lock failed".to_string())?
        .clone();
    if !unsanitized {
        for root in &mut scan.roots {
            sanitize_node(root);
        }
        for entry in &mut persisted.history {
            entry.deleted_paths = entry
                .deleted_paths
                .iter()
                .map(|path| sanitize_path(Path::new(path)))
                .collect();
            for outcome in &mut entry.outcomes {
                outcome.path = outcome.display_path.clone();
            }
        }
        if !persisted.settings.monitored_volume.is_empty() {
            persisted.settings.monitored_volume =
                sanitize_path(Path::new(&persisted.settings.monitored_volume));
        }
    }
    let report = serde_json::json!({
        "generated_at": Local::now().to_rfc3339(),
        "paths_sanitized": !unsanitized,
        "privacy_warning": if unsanitized { Some("This diagnostic export contains full local filesystem paths.") } else { None },
        "scan": scan,
        "settings": persisted.settings,
        "history": persisted.history,
        "growth_alert": calculate_growth_alert(&persisted),
    });
    let dir = report_directory(&app);
    let path = dir.join(format!(
        "claude-cache-report-{}.json",
        Local::now().format("%Y%m%d-%H%M%S")
    ));
    fs::write(
        &path,
        serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

fn sanitize_node(node: &mut crate::models::CacheNode) {
    node.path = sanitize_path(Path::new(&node.path));
    for child in &mut node.children {
        sanitize_node(child);
    }
}
