use chrono::{DateTime, Local, NaiveTime};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, SystemTime},
};
use sysinfo::System;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, State,
};

#[derive(Debug, Default)]
struct RemoveStats {
    removed_entries: u64,
    errors: Vec<String>,
}

const MAX_SAMPLES: usize = 96;
const DEFAULT_GROWTH_ALERT_GB_PER_HOUR: f64 = 2.0;
const SCHEDULER_TICK_SECONDS: u64 = 60;
const SCHEDULER_SCAN_INTERVAL: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
enum SafetyLevel {
    Safe,
    Caution,
    NotRecommended,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ClaudeActivity {
    NotDetected,
    Background,
    Window,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheNode {
    label: String,
    path: String,
    size_bytes: u64,
    file_count: u64,
    dir_count: u64,
    exists: bool,
    safety: SafetyLevel,
    default_cleanup: bool,
    description: String,
    children: Vec<CacheNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScanResult {
    platform: String,
    scanned_at: String,
    total_bytes: u64,
    roots: Vec<CacheNode>,
    claude_running: bool,
    claude_activity: ClaudeActivity,
    warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct CleanRequest {
    paths: Vec<String>,
    allow_when_running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CleanResult {
    cleaned_bytes: u64,
    deleted_paths: Vec<String>,
    skipped_paths: Vec<String>,
    errors: Vec<String>,
    remaining_bytes: u64,
    cleaned_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SchedulerSettings {
    enabled: bool,
    schedule_enabled: bool,
    schedule_time: String,
    threshold_enabled: bool,
    threshold_gb: f64,
    growth_alert_enabled: bool,
    growth_alert_gb_per_hour: f64,
    clean_when_claude_running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CleanHistoryEntry {
    cleaned_at: String,
    cleaned_bytes: u64,
    remaining_bytes: u64,
    trigger: String,
    deleted_paths: Vec<String>,
    errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SizeSample {
    captured_at: DateTime<Local>,
    total_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GrowthAlert {
    active: bool,
    gb_per_hour: f64,
    baseline_gb_per_hour: f64,
    message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedState {
    settings: SchedulerSettings,
    history: Vec<CleanHistoryEntry>,
    samples: VecDeque<SizeSample>,
    last_schedule_day: Option<String>,
}

struct AppState {
    persisted: Mutex<PersistedState>,
    scheduler_scan_cache: Mutex<Option<SchedulerScanCache>>,
    storage_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RootSignature {
    path: PathBuf,
    modified: Option<SystemTime>,
}

#[derive(Debug, Clone)]
struct SchedulerScanCache {
    scanned_at: DateTime<Local>,
    root_signature: Vec<RootSignature>,
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let state = AppState::load(app.handle().clone());
            app.manage(Arc::new(state));
            build_tray(app)?;
            start_scheduler(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            scan_cache,
            clean_cache,
            force_clean_cache,
            get_scheduler_settings,
            save_scheduler_settings,
            get_clean_history,
            evaluate_growth_alert,
            export_report,
            get_claude_running,
            get_claude_activity
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Claude Cache Warden");
}

impl Default for SchedulerSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            schedule_enabled: true,
            schedule_time: "02:00".to_string(),
            threshold_enabled: true,
            threshold_gb: 5.0,
            growth_alert_enabled: true,
            growth_alert_gb_per_hour: DEFAULT_GROWTH_ALERT_GB_PER_HOUR,
            clean_when_claude_running: false,
        }
    }
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            settings: SchedulerSettings::default(),
            history: Vec::new(),
            samples: VecDeque::new(),
            last_schedule_day: None,
        }
    }
}

impl AppState {
    fn load(app: tauri::AppHandle) -> Self {
        let base_dir = app.path().app_config_dir().unwrap_or_else(|_| {
            dirs::config_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("ClaudeCacheWarden")
        });
        let _ = fs::create_dir_all(&base_dir);
        let storage_path = base_dir.join("state.json");
        let persisted = fs::read_to_string(&storage_path)
            .ok()
            .and_then(|value| serde_json::from_str::<PersistedState>(&value).ok())
            .unwrap_or_default();

        Self {
            persisted: Mutex::new(persisted),
            scheduler_scan_cache: Mutex::new(None),
            storage_path,
        }
    }

    fn save(&self) -> Result<(), String> {
        let state = self
            .persisted
            .lock()
            .map_err(|_| "State lock failed".to_string())?;
        let value = serde_json::to_string_pretty(&*state).map_err(|error| error.to_string())?;
        fs::write(&self.storage_path, value).map_err(|error| error.to_string())
    }

    fn record_sample(&self, total_bytes: u64) -> Result<(), String> {
        {
            let mut state = self
                .persisted
                .lock()
                .map_err(|_| "State lock failed".to_string())?;
            state.samples.push_back(SizeSample {
                captured_at: Local::now(),
                total_bytes,
            });
            while state.samples.len() > MAX_SAMPLES {
                state.samples.pop_front();
            }
        }
        self.save()
    }

    fn push_history(&self, entry: CleanHistoryEntry) -> Result<(), String> {
        {
            let mut state = self
                .persisted
                .lock()
                .map_err(|_| "State lock failed".to_string())?;
            state.history.insert(0, entry);
            state.history.truncate(100);
        }
        self.save()
    }
}

fn build_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let show_i = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let scan_i = MenuItem::with_id(app, "scan", "Scan", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_i, &scan_i, &quit_i])?;
    let tray_icon = app.default_window_icon().cloned();

    let mut builder = TrayIconBuilder::new();
    if let Some(icon) = tray_icon {
        builder = builder.icon(icon);
    }

    builder
        .tooltip("Claude Cache Warden")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "scan" => {
                let _ = app.emit("scan-requested", ());
                show_main_window(app);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(&tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[tauri::command]
fn scan_cache(state: State<'_, Arc<AppState>>) -> Result<ScanResult, String> {
    let result = perform_scan()?;
    let _ = state.record_sample(result.total_bytes);
    Ok(result)
}

#[tauri::command]
fn clean_cache(
    state: State<'_, Arc<AppState>>,
    request: CleanRequest,
) -> Result<CleanResult, String> {
    let activity = claude_activity();
    if claude_activity_blocks_cleanup(activity) && !request.allow_when_running {
        return Err(claude_cleanup_block_message(activity));
    }

    let result = perform_clean(&request.paths)?;
    state.push_history(CleanHistoryEntry {
        cleaned_at: result.cleaned_at.clone(),
        cleaned_bytes: result.cleaned_bytes,
        remaining_bytes: result.remaining_bytes,
        trigger: "manual".to_string(),
        deleted_paths: result.deleted_paths.clone(),
        errors: result.errors.clone(),
    })?;
    Ok(result)
}

#[tauri::command]
fn force_clean_cache(
    state: State<'_, Arc<AppState>>,
    request: CleanRequest,
) -> Result<CleanResult, String> {
    if request.paths.len() != 1 {
        return Err("Force cleanup accepts exactly one path.".to_string());
    }

    let activity = claude_activity();
    if claude_activity_blocks_cleanup(activity) && !request.allow_when_running {
        return Err(claude_cleanup_block_message(activity));
    }

    let result = perform_forced_clean(&request.paths)?;
    state.push_history(CleanHistoryEntry {
        cleaned_at: result.cleaned_at.clone(),
        cleaned_bytes: result.cleaned_bytes,
        remaining_bytes: result.remaining_bytes,
        trigger: "manual-force".to_string(),
        deleted_paths: result.deleted_paths.clone(),
        errors: result.errors.clone(),
    })?;
    Ok(result)
}

#[tauri::command]
fn get_scheduler_settings(state: State<'_, Arc<AppState>>) -> Result<SchedulerSettings, String> {
    let state = state
        .persisted
        .lock()
        .map_err(|_| "State lock failed".to_string())?;
    Ok(state.settings.clone())
}

#[tauri::command]
fn save_scheduler_settings(
    state: State<'_, Arc<AppState>>,
    settings: SchedulerSettings,
) -> Result<SchedulerSettings, String> {
    {
        let mut persisted = state
            .persisted
            .lock()
            .map_err(|_| "State lock failed".to_string())?;
        persisted.settings = normalize_settings(settings);
    }
    state.save()?;
    let persisted = state
        .persisted
        .lock()
        .map_err(|_| "State lock failed".to_string())?;
    Ok(persisted.settings.clone())
}

#[tauri::command]
fn get_clean_history(state: State<'_, Arc<AppState>>) -> Result<Vec<CleanHistoryEntry>, String> {
    let state = state
        .persisted
        .lock()
        .map_err(|_| "State lock failed".to_string())?;
    Ok(state.history.clone())
}

#[tauri::command]
fn evaluate_growth_alert(state: State<'_, Arc<AppState>>) -> Result<GrowthAlert, String> {
    let state = state
        .persisted
        .lock()
        .map_err(|_| "State lock failed".to_string())?;
    Ok(calculate_growth_alert(&state))
}

#[tauri::command]
fn export_report(app: tauri::AppHandle, state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let scan = perform_scan()?;
    let persisted = state
        .persisted
        .lock()
        .map_err(|_| "State lock failed".to_string())?
        .clone();
    let report = serde_json::json!({
        "generated_at": Local::now().to_rfc3339(),
        "scan": scan,
        "settings": persisted.settings,
        "history": persisted.history,
        "growth_alert": calculate_growth_alert(&persisted),
    });

    let dir = app
        .path()
        .document_dir()
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir()));
    let path = dir.join(format!(
        "claude-cache-report-{}.json",
        Local::now().format("%Y%m%d-%H%M%S")
    ));
    fs::write(
        &path,
        serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
fn get_claude_running() -> bool {
    is_claude_running()
}

#[tauri::command]
fn get_claude_activity() -> ClaudeActivity {
    claude_activity()
}

fn start_scheduler(app: tauri::AppHandle) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(SCHEDULER_TICK_SECONDS));
        let Some(state_ref) = app.try_state::<Arc<AppState>>() else {
            continue;
        };
        let state = state_ref.inner().clone();

        let should_run = match should_scheduler_run(&state) {
            Ok(value) => value,
            Err(_) => continue,
        };

        if let Some(trigger) = should_run {
            let settings = match state.persisted.lock() {
                Ok(value) => value.settings.clone(),
                Err(_) => continue,
            };
            if claude_activity_blocks_cleanup(claude_activity())
                && !settings.clean_when_claude_running
            {
                continue;
            }
            let paths = safe_cleanup_paths();
            if let Ok(result) = perform_clean(&paths) {
                let _ = state.push_history(CleanHistoryEntry {
                    cleaned_at: result.cleaned_at.clone(),
                    cleaned_bytes: result.cleaned_bytes,
                    remaining_bytes: result.remaining_bytes,
                    trigger,
                    deleted_paths: result.deleted_paths,
                    errors: result.errors,
                });
                let _ = app.emit("cleanup-completed", ());
            }
        }
    });
}

fn should_scheduler_run(state: &Arc<AppState>) -> Result<Option<String>, String> {
    let now = Local::now();
    let today = now.format("%Y-%m-%d").to_string();
    let settings = {
        let persisted = state
            .persisted
            .lock()
            .map_err(|_| "State lock failed".to_string())?;
        let settings = persisted.settings.clone();
        if !settings.enabled {
            // When automation is disabled, avoid all recursive disk scans.
            return Ok(None);
        }
        settings
    };

    if settings.schedule_enabled {
        let time = NaiveTime::parse_from_str(&settings.schedule_time, "%H:%M")
            .unwrap_or_else(|_| NaiveTime::from_hms_opt(2, 0, 0).unwrap());
        let now_time = now.time();
        let should_run_schedule = {
            let persisted = state
                .persisted
                .lock()
                .map_err(|_| "State lock failed".to_string())?;
            now_time.hour() == time.hour()
                && now_time.minute() == time.minute()
                && persisted.last_schedule_day.as_deref() != Some(&today)
        };
        if should_run_schedule {
            let mut persisted = state
                .persisted
                .lock()
                .map_err(|_| "State lock failed".to_string())?;
            persisted.last_schedule_day = Some(today);
            drop(persisted);
            state.save()?;
            return Ok(Some("schedule".to_string()));
        }
    }

    if !settings.threshold_enabled && !settings.growth_alert_enabled {
        return Ok(None);
    }

    let Some(root_signature) = next_scheduler_scan_signature(state)? else {
        return Ok(None);
    };

    let scan = perform_scan()?;
    let _ = state.record_sample(scan.total_bytes);
    mark_scheduler_scan_complete(state, root_signature)?;

    if settings.threshold_enabled && bytes_to_gb(scan.total_bytes) >= settings.threshold_gb {
        return Ok(Some("threshold".to_string()));
    }

    Ok(None)
}

fn next_scheduler_scan_signature(
    state: &Arc<AppState>,
) -> Result<Option<Vec<RootSignature>>, String> {
    let root_signature = root_modification_signature();
    let cache = state
        .scheduler_scan_cache
        .lock()
        .map_err(|_| "Scheduler cache lock failed".to_string())?;
    let Some(cache) = cache.as_ref() else {
        return Ok(Some(root_signature));
    };

    let elapsed = (Local::now() - cache.scanned_at)
        .to_std()
        .unwrap_or_else(|_| Duration::from_secs(0));
    if elapsed < SCHEDULER_SCAN_INTERVAL && cache.root_signature == root_signature {
        // Root mtime is unchanged and the last full scan is fresh enough.
        return Ok(None);
    }

    Ok(Some(root_signature))
}

fn mark_scheduler_scan_complete(
    state: &Arc<AppState>,
    root_signature: Vec<RootSignature>,
) -> Result<(), String> {
    let mut cache = state
        .scheduler_scan_cache
        .lock()
        .map_err(|_| "Scheduler cache lock failed".to_string())?;
    *cache = Some(SchedulerScanCache {
        scanned_at: Local::now(),
        root_signature,
    });
    Ok(())
}

fn perform_scan() -> Result<ScanResult, String> {
    let roots = claude_roots();
    let mut warnings = Vec::new();
    if roots.is_empty() {
        warnings.push("No Claude cache root could be resolved for this platform.".to_string());
    }
    warnings.extend(windows_root_debug_warnings(&roots));

    let mut scanned_roots = Vec::new();
    for root in roots {
        scanned_roots.push(
            scan_path(
                &root.path,
                root.label,
                root.safety,
                root.default_cleanup,
                root.description,
            )
            .map_err(|error| error.to_string())?,
        );
    }

    let total_bytes = scanned_roots.iter().map(|node| node.size_bytes).sum();
    let activity = claude_activity();
    Ok(ScanResult {
        platform: std::env::consts::OS.to_string(),
        scanned_at: Local::now().to_rfc3339(),
        total_bytes,
        roots: scanned_roots,
        claude_running: activity == ClaudeActivity::Window,
        claude_activity: activity,
        warnings,
    })
}

fn perform_clean(paths: &[String]) -> Result<CleanResult, String> {
    perform_clean_with_policy(paths, true)
}

fn perform_forced_clean(paths: &[String]) -> Result<CleanResult, String> {
    perform_clean_with_policy(paths, false)
}

fn perform_clean_with_policy(
    paths: &[String],
    require_safe_target: bool,
) -> Result<CleanResult, String> {
    let cleaned_at = Local::now().to_rfc3339();
    let mut cleaned_bytes = 0;
    let mut deleted_paths = Vec::new();
    let mut skipped_paths = Vec::new();
    let mut errors = Vec::new();

    if paths.is_empty() {
        return Err("No cleanup paths were selected.".to_string());
    }

    for raw in paths {
        let path = PathBuf::from(raw);
        if !path.exists() {
            skipped_paths.push(raw.clone());
            continue;
        }
        if !is_allowed_cleanup_target(&path) {
            skipped_paths.push(raw.clone());
            errors.push(format!(
                "Refused to clean outside known Claude cache roots: {}",
                raw
            ));
            continue;
        }
        if require_safe_target && !is_cleanup_safe_target(&path) {
            skipped_paths.push(raw.clone());
            errors.push(format!("Refused to clean non-safe target: {}", raw));
            continue;
        }

        let before_size = dir_size(&path).unwrap_or(0);
        let stats = remove_path_contents_or_file(&path);
        let after_size = if path.exists() {
            dir_size(&path).unwrap_or(0)
        } else {
            0
        };
        let freed_bytes = before_size.saturating_sub(after_size);

        if freed_bytes > 0 || stats.removed_entries > 0 {
            cleaned_bytes += freed_bytes;
            deleted_paths.push(raw.clone());
        } else {
            skipped_paths.push(raw.clone());
        }

        if before_size > 0
            && freed_bytes == 0
            && stats.removed_entries == 0
            && stats.errors.is_empty()
        {
            errors.push(format!("{}: no files were removed", raw));
        }

        errors.extend(
            stats
                .errors
                .into_iter()
                .map(|error| format!("{}: {}", raw, error)),
        );
    }

    if cleaned_bytes == 0 && deleted_paths.is_empty() && !errors.is_empty() {
        return Err(format_cleanup_errors(&errors));
    }

    if cleaned_bytes == 0 && deleted_paths.is_empty() && skipped_paths.len() == paths.len() {
        return Err("Cleanup did not remove any files from the selected paths.".to_string());
    }

    let remaining_bytes = perform_scan().map(|scan| scan.total_bytes).unwrap_or(0);
    Ok(CleanResult {
        cleaned_bytes,
        deleted_paths,
        skipped_paths,
        errors,
        remaining_bytes,
        cleaned_at,
    })
}

fn format_cleanup_errors(errors: &[String]) -> String {
    const MAX_ERRORS: usize = 5;
    let mut message = "Cleanup did not remove any files.".to_string();
    for error in errors.iter().take(MAX_ERRORS) {
        message.push_str("\n- ");
        message.push_str(error);
    }
    if errors.len() > MAX_ERRORS {
        message.push_str(&format!(
            "\n- ... and {} more error(s)",
            errors.len() - MAX_ERRORS
        ));
    }
    message
}

fn remove_path_contents_or_file(path: &Path) -> RemoveStats {
    let mut stats = RemoveStats::default();
    let Ok(metadata) = fs::symlink_metadata(path) else {
        stats
            .errors
            .push(format!("Cannot read metadata for {}", path.display()));
        return stats;
    };

    if metadata.file_type().is_symlink() {
        stats
            .errors
            .push(format!("Skipped symlink {}", path.display()));
        return stats;
    }

    if metadata.is_file() {
        remove_file_entry(path, &mut stats);
        return stats;
    }

    if is_claude_vm_bundle_leaf(path) {
        remove_vm_bundle_cache_contents(path, &mut stats);
        return stats;
    }

    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) => {
            stats.errors.push(format!(
                "Cannot read directory {}: {}",
                path.display(),
                error
            ));
            return stats;
        }
    };

    for entry in entries {
        match entry {
            Ok(entry) => remove_entry_tree(&entry.path(), &mut stats),
            Err(error) => stats.errors.push(format!(
                "Cannot read directory entry in {}: {}",
                path.display(),
                error
            )),
        }
    }

    stats
}

fn remove_vm_bundle_cache_contents(path: &Path, stats: &mut RemoveStats) {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) => {
            stats.errors.push(format!(
                "Cannot read VM bundle directory {}: {}",
                path.display(),
                error
            ));
            return;
        }
    };

    for entry in entries {
        match entry {
            Ok(entry) => {
                let child_path = entry.path();
                if is_rebuildable_vm_bundle_artifact(&child_path) {
                    remove_entry_tree(&child_path, stats);
                }
            }
            Err(error) => stats.errors.push(format!(
                "Cannot read VM bundle entry in {}: {}",
                path.display(),
                error
            )),
        }
    }
}

fn scan_path(
    path: &Path,
    label: String,
    safety: SafetyLevel,
    default_cleanup: bool,
    description: String,
) -> io::Result<CacheNode> {
    if !path.exists() {
        return Ok(CacheNode {
            label,
            path: path.to_string_lossy().to_string(),
            size_bytes: 0,
            file_count: 0,
            dir_count: 0,
            exists: false,
            safety,
            default_cleanup,
            description,
            children: Vec::new(),
        });
    }

    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_file() {
        return Ok(CacheNode {
            label,
            path: path.to_string_lossy().to_string(),
            size_bytes: metadata.len(),
            file_count: 1,
            dir_count: 0,
            exists: true,
            safety,
            default_cleanup,
            description,
            children: Vec::new(),
        });
    }

    let mut size_bytes = 0;
    let mut file_count = 0;
    let mut dir_count = 0;
    let mut children = Vec::new();

    for entry in fs::read_dir(path)? {
        let entry = match entry {
            Ok(value) => value,
            Err(_) => continue,
        };
        let child_path = entry.path();
        let child_meta = match fs::symlink_metadata(&child_path) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if child_meta.file_type().is_symlink() {
            continue;
        }
        if child_meta.is_dir() {
            let (child_safety, child_description) = classify_path(&child_path);
            let child_default_cleanup = is_cleanup_safe_target(&child_path);
            let child_label = display_label(&child_path);
            let child = scan_path(
                &child_path,
                child_label,
                child_safety,
                child_default_cleanup,
                child_description,
            )?;
            size_bytes += child.size_bytes;
            file_count += child.file_count;
            dir_count += child.dir_count + 1;
            children.push(child);
        } else if child_meta.is_file() {
            size_bytes += child_meta.len();
            file_count += 1;
        }
    }

    children.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    Ok(CacheNode {
        label,
        path: path.to_string_lossy().to_string(),
        size_bytes,
        file_count,
        dir_count,
        exists: true,
        safety,
        default_cleanup,
        description,
        children,
    })
}

fn claude_roots() -> Vec<KnownRoot> {
    match std::env::consts::OS {
        "macos" => dirs::home_dir()
            .map(|home| {
                vec![
                    root(
                        home.join("Library/Application Support/Claude/vm_bundles"),
                        "Claude workspace bundles",
                    ),
                    root(
                        home.join("Library/Application Support/Claude/vm_bundles/warm"),
                        "Warm VM bundles",
                    ),
                    root(
                        home.join("Library/Application Support/Claude/Cache"),
                        "Renderer cache",
                    ),
                    root(
                        home.join("Library/Application Support/Claude/Code Cache"),
                        "Code cache",
                    ),
                    root(
                        home.join("Library/Application Support/Claude/claude-code-vm"),
                        "Claude Code VM cache",
                    ),
                    root(
                        home.join("Library/Application Support/Claude/claude-code"),
                        "Claude Code cache",
                    ),
                    root(home.join("Library/Caches/Claude"), "Claude system cache"),
                ]
            })
            .unwrap_or_default(),
        "windows" => {
            let mut roots = Vec::new();
            if let Ok(appdata) = std::env::var("APPDATA") {
                roots.push(root(
                    PathBuf::from(appdata).join("Claude"),
                    "Claude roaming data",
                ));
            }
            if let Ok(local) = std::env::var("LOCALAPPDATA") {
                let local_path = PathBuf::from(local);
                roots.push(root(local_path.join("Claude"), "Claude local cache"));
                roots.push(root(
                    local_path.join("Claude-3p"),
                    "Claude (3p channel) data",
                ));
                roots.push(root(
                    local_path.join("Temp").join("claude"),
                    "Claude temp files",
                ));
                roots.extend(windows_store_package_roots(&local_path));
            }
            roots
        }
        _ => Vec::new(),
    }
}

fn root_modification_signature() -> Vec<RootSignature> {
    claude_roots()
        .into_iter()
        .map(|root| RootSignature {
            modified: fs::metadata(&root.path)
                .and_then(|metadata| metadata.modified())
                .ok(),
            path: root.path,
        })
        .collect()
}

fn windows_root_debug_warnings(roots: &[KnownRoot]) -> Vec<String> {
    if std::env::consts::OS != "windows" || !windows_root_debug_enabled() {
        return Vec::new();
    }

    let mut warnings = Vec::new();
    for root in roots {
        let child_names = direct_child_dir_names(&root.path);
        let message = match child_names {
            Ok(names) if names.is_empty() => {
                format!(
                    "Debug: {} has no direct child directories.",
                    root.path.display()
                )
            }
            Ok(names) => format!(
                "Debug: {} child directories: {}",
                root.path.display(),
                names.join(", ")
            ),
            Err(error) => format!(
                "Debug: {} could not be read: {}",
                root.path.display(),
                error
            ),
        };
        eprintln!("{message}");
        warnings.push(message);

        if is_path_leaf(&root.path, "LocalCache") && root.path.exists() {
            let message = match direct_child_dir_sizes(&root.path) {
                Ok(sizes) if sizes.is_empty() => {
                    format!(
                        "Debug: {} has no child directory sizes to report.",
                        root.path.display()
                    )
                }
                Ok(sizes) => {
                    let formatted = sizes
                        .into_iter()
                        .map(|(name, bytes)| format!("{}={}", name, format_debug_bytes(bytes)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!(
                        "Debug: {} child directory sizes: {}",
                        root.path.display(),
                        formatted
                    )
                }
                Err(error) => format!(
                    "Debug: {} child directory sizes could not be read: {}",
                    root.path.display(),
                    error
                ),
            };
            eprintln!("{message}");
            warnings.push(message);
        }
    }

    warnings
}

fn windows_root_debug_enabled() -> bool {
    cfg!(debug_assertions)
        || std::env::var("CCW_DEBUG_WINDOWS_ROOTS")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
}

fn direct_child_dir_names(path: &Path) -> io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            names.push(entry.file_name().to_string_lossy().to_string());
        }
    }
    names.sort();
    Ok(names)
}

fn direct_child_dir_sizes(path: &Path) -> io::Result<Vec<(String, u64)>> {
    let mut sizes = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            let size = dir_size(&entry.path()).unwrap_or(0);
            sizes.push((name, size));
        }
    }
    sizes.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Ok(sizes)
}

fn root(path: PathBuf, label: &str) -> KnownRoot {
    let (safety, description) = classify_path(&path);
    known_root(
        path,
        label,
        safety,
        safety == SafetyLevel::Safe,
        description,
    )
}

fn review_root(path: PathBuf, label: &str) -> KnownRoot {
    let (safety, description) = classify_path(&path);
    known_root(path, label, safety, false, description)
}

fn known_root(
    path: PathBuf,
    label: &str,
    safety: SafetyLevel,
    default_cleanup: bool,
    description: String,
) -> KnownRoot {
    KnownRoot {
        path,
        label: label.to_string(),
        safety,
        default_cleanup,
        description,
    }
}

fn windows_store_package_roots(local_path: &Path) -> Vec<KnownRoot> {
    let packages_path = local_path.join("Packages");
    let Ok(entries) = fs::read_dir(packages_path) else {
        return Vec::new();
    };

    let mut package_roots = entries
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_type()
                .map(|file_type| file_type.is_dir())
                .unwrap_or(false)
        })
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .starts_with("claude_")
        })
        .map(|entry| entry.path())
        .collect::<Vec<_>>();

    package_roots.sort();
    package_roots
        .into_iter()
        .flat_map(|package_root| windows_store_package_branch_roots(&package_root))
        .collect()
}

fn windows_store_package_branch_roots(package_root: &Path) -> Vec<KnownRoot> {
    vec![
        review_root(
            package_root.join("LocalCache"),
            "Claude Store package cache",
        ),
        root(
            package_root.join("TempState"),
            "Claude Store package temp state",
        ),
        review_root(
            package_root.join("LocalState"),
            "Claude Store package local state",
        ),
        review_root(
            package_root.join("RoamingState"),
            "Claude Store package roaming state",
        ),
        review_root(
            package_root.join("Settings"),
            "Claude Store package settings",
        ),
        review_root(
            package_root.join("AC"),
            "Claude Store package app container",
        ),
        review_root(
            package_root.join("SystemAppData"),
            "Claude Store package system app data",
        ),
    ]
}

struct KnownRoot {
    path: PathBuf,
    label: String,
    safety: SafetyLevel,
    default_cleanup: bool,
    description: String,
}

fn classify_path(path: &Path) -> (SafetyLevel, String) {
    let normalized = path.to_string_lossy().to_ascii_lowercase();
    if is_path_leaf(path, "LocalCache") {
        (
            SafetyLevel::NotRecommended,
            "Contains cache plus Claude workspace, session, and app-state data. Do not clean the whole LocalCache root.".to_string(),
        )
    } else if is_rebuildable_vm_bundle_leaf(path) {
        (
            SafetyLevel::Safe,
            "Rebuildable Claude VM runtime bundle cache. Claude can recreate it after cleanup."
                .to_string(),
        )
    } else if has_sensitive_state_segment(path) {
        (
            SafetyLevel::NotRecommended,
            "May contain Claude projects, sessions, identity, browser storage, or workspace runtime data.".to_string(),
        )
    } else if is_path_leaf(path, "TempState") {
        (
            SafetyLevel::Safe,
            "Claude Store package temporary state. Review debug output before enabling automatic cleanup.".to_string(),
        )
    } else if is_path_leaf(path, "LocalState")
        || is_path_leaf(path, "RoamingState")
        || is_path_leaf(path, "Settings")
    {
        (
            SafetyLevel::NotRecommended,
            "May contain Store app state, settings, or session data.".to_string(),
        )
    } else if is_path_leaf(path, "AC") || is_path_leaf(path, "SystemAppData") {
        (
            SafetyLevel::Caution,
            "Windows app-container data. Review this location before deleting.".to_string(),
        )
    } else if normalized.ends_with("\\temp\\claude")
        || normalized.ends_with("/temp/claude")
        || is_rebuildable_cache_leaf(path)
        || normalized.ends_with("\\library\\caches\\claude")
        || normalized.ends_with("/library/caches/claude")
    {
        (
            SafetyLevel::Safe,
            "Cache data that Claude can rebuild after cleanup.".to_string(),
        )
    } else if normalized.contains("session")
        || normalized.contains("config")
        || normalized.contains("preferences")
        || normalized.ends_with("\\claude")
        || normalized.ends_with("/claude")
    {
        (
            SafetyLevel::NotRecommended,
            "May contain settings, session data, or the top-level Claude data folder.".to_string(),
        )
    } else {
        (
            SafetyLevel::Caution,
            "Review this location before deleting because it may be tied to active workspaces."
                .to_string(),
        )
    }
}

fn safe_cleanup_paths() -> Vec<String> {
    let mut paths = Vec::new();
    for root in claude_roots() {
        if let Ok(node) = scan_path(
            &root.path,
            root.label,
            root.safety,
            root.default_cleanup,
            root.description,
        ) {
            collect_default_cleanup_paths(&node, &mut paths);
        }
    }
    paths
}

fn collect_default_cleanup_paths(node: &CacheNode, paths: &mut Vec<String>) {
    if node.exists && node.default_cleanup && node.size_bytes > 0 {
        paths.push(node.path.clone());
        return;
    }

    for child in &node.children {
        collect_default_cleanup_paths(child, paths);
    }
}

fn is_cleanup_safe_target(path: &Path) -> bool {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return false;
        }
    }

    if is_rebuildable_vm_bundle_leaf(path) {
        return true;
    }

    if has_sensitive_state_segment(path) {
        return false;
    }

    classify_path(path).0 == SafetyLevel::Safe
}

fn is_allowed_cleanup_target(path: &Path) -> bool {
    let Ok(candidate) = path.canonicalize() else {
        return false;
    };

    claude_roots().into_iter().any(|root| {
        root.path
            .canonicalize()
            .map(|known| candidate == known || candidate.starts_with(&known))
            .unwrap_or(false)
    })
}

fn remove_entry_tree(path: &Path, stats: &mut RemoveStats) {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            stats.errors.push(format!(
                "Cannot read metadata for {}: {}",
                path.display(),
                error
            ));
            return;
        }
    };

    if metadata.file_type().is_symlink() {
        stats
            .errors
            .push(format!("Skipped symlink {}", path.display()));
        return;
    }

    if metadata.is_file() {
        remove_file_entry(path, stats);
        return;
    }

    if metadata.is_dir() {
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error) => {
                stats.errors.push(format!(
                    "Cannot read directory {}: {}",
                    path.display(),
                    error
                ));
                return;
            }
        };

        for entry in entries {
            match entry {
                Ok(entry) => remove_entry_tree(&entry.path(), stats),
                Err(error) => stats.errors.push(format!(
                    "Cannot read directory entry in {}: {}",
                    path.display(),
                    error
                )),
            }
        }

        remove_dir_entry(path, stats);
    }
}

fn remove_file_entry(path: &Path, stats: &mut RemoveStats) {
    match fs::remove_file(path) {
        Ok(()) => stats.removed_entries += 1,
        Err(first_error) => {
            make_writable(path);
            match fs::remove_file(path) {
                Ok(()) => stats.removed_entries += 1,
                Err(second_error) => stats.errors.push(format!(
                    "Cannot remove file {}: {}; retry after writable: {}",
                    path.display(),
                    first_error,
                    second_error
                )),
            }
        }
    }
}

fn remove_dir_entry(path: &Path, stats: &mut RemoveStats) {
    match fs::remove_dir(path) {
        Ok(()) => stats.removed_entries += 1,
        Err(first_error) => {
            make_writable(path);
            match fs::remove_dir(path) {
                Ok(()) => stats.removed_entries += 1,
                Err(second_error) => stats.errors.push(format!(
                    "Cannot remove directory {}: {}; retry after writable: {}",
                    path.display(),
                    first_error,
                    second_error
                )),
            }
        }
    }
}

fn make_writable(path: &Path) {
    if let Ok(metadata) = fs::metadata(path) {
        let mut permissions = metadata.permissions();
        if permissions.readonly() {
            permissions.set_readonly(false);
            let _ = fs::set_permissions(path, permissions);
        }
    }
}

fn dir_size(path: &Path) -> io::Result<u64> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Ok(0);
    }
    if metadata.is_file() {
        return Ok(metadata.len());
    }

    let mut total = 0;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        total += dir_size(&entry.path()).unwrap_or(0);
    }
    Ok(total)
}

fn display_label(path: &Path) -> String {
    path.file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

fn is_path_leaf(path: &Path, expected: &str) -> bool {
    path.file_name()
        .map(|value| value.to_string_lossy().eq_ignore_ascii_case(expected))
        .unwrap_or(false)
}

fn is_rebuildable_cache_leaf(path: &Path) -> bool {
    [
        "Cache",
        "Code Cache",
        "GPUCache",
        "DawnGraphiteCache",
        "DawnWebGPUCache",
    ]
    .iter()
    .any(|leaf| is_path_leaf(path, leaf))
}

fn is_rebuildable_vm_bundle_leaf(path: &Path) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    if !is_path_leaf(parent, "vm_bundles") {
        return false;
    }

    path.file_name()
        .map(|value| {
            let leaf = value.to_string_lossy().to_ascii_lowercase();
            leaf == "claudevm.bundle" || leaf == "warm"
        })
        .unwrap_or(false)
}

fn is_claude_vm_bundle_leaf(path: &Path) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    is_path_leaf(parent, "vm_bundles") && is_path_leaf(path, "claudevm.bundle")
}

fn is_rebuildable_vm_bundle_artifact(path: &Path) -> bool {
    path.file_name()
        .map(|value| {
            matches!(
                value.to_string_lossy().to_ascii_lowercase().as_str(),
                "rootfs.vhdx"
                    | "rootfs.vhdx.zst"
                    | "initrd"
                    | "initrd.zst"
                    | "vmlinuz"
                    | "vmlinuz.zst"
                    | "smol-bin.vhdx"
            )
        })
        .unwrap_or(false)
}

fn has_sensitive_state_segment(path: &Path) -> bool {
    path.components().any(|component| {
        let segment = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        matches!(
            segment.as_str(),
            "indexeddb"
                | "local storage"
                | "session storage"
                | "file system"
                | "service worker"
                | "network"
                | "partitions"
                | "cookies"
                | "local-agent-mode-sessions"
                | "vm_bundles"
                | "claude-code"
                | "claude-code-vm"
                | "settings"
                | "localstate"
                | "roamingstate"
                | "systemappdata"
                | "config"
                | "preferences"
                | "local state"
                | "config.json"
                | "claude_desktop_config.json"
                | "git-worktrees.json"
                | "buddy-tokens.json"
                | "cowork-enabled-cli-ops.json"
                | "ant-did"
                | "dips"
                | "dips-wal"
                | "sharedstorage"
                | "sharedstorage-wal"
                | "shared dictionary"
                | "webstorage"
        )
    })
}

fn format_debug_bytes(bytes: u64) -> String {
    const GIB: u64 = 1024 * 1024 * 1024;
    const MIB: u64 = 1024 * 1024;
    if bytes >= GIB {
        format!("{:.2} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}

fn claude_activity_blocks_cleanup(activity: ClaudeActivity) -> bool {
    activity != ClaudeActivity::NotDetected
}

fn claude_cleanup_block_message(activity: ClaudeActivity) -> String {
    match activity {
        ClaudeActivity::Background => {
            "Claude background processes are still running and may be locking cache files. Fully quit Claude from the tray or Task Manager, then scan again.".to_string()
        }
        ClaudeActivity::Window => {
            "Claude Desktop is running. Close Claude or allow cleanup while it is running.".to_string()
        }
        ClaudeActivity::NotDetected => "Claude is not detected.".to_string(),
    }
}

fn is_claude_running() -> bool {
    claude_activity() == ClaudeActivity::Window
}

fn claude_activity() -> ClaudeActivity {
    let mut system = System::new_all();
    system.refresh_processes();
    let current_pid = std::process::id().to_string();

    match std::env::consts::OS {
        "windows" => {
            let process_ids = system
                .processes()
                .iter()
                .filter_map(|(pid, process)| {
                    if is_claude_desktop_process_name(
                        process.name(),
                        &pid.to_string(),
                        &current_pid,
                    ) {
                        pid.to_string().parse::<u32>().ok()
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            if process_ids.is_empty() {
                ClaudeActivity::NotDetected
            } else if windows_has_visible_window_for_processes(&process_ids) {
                ClaudeActivity::Window
            } else {
                ClaudeActivity::Background
            }
        }
        _ => {
            let detected = system.processes().iter().any(|(pid, process)| {
                is_claude_desktop_process_name(process.name(), &pid.to_string(), &current_pid)
            });
            if detected {
                ClaudeActivity::Window
            } else {
                ClaudeActivity::NotDetected
            }
        }
    }
}

fn is_claude_desktop_process_name(
    process_name: &str,
    process_pid: &str,
    current_pid: &str,
) -> bool {
    is_claude_desktop_process_name_for_os(
        process_name,
        process_pid,
        current_pid,
        std::env::consts::OS,
    )
}

fn is_claude_desktop_process_name_for_os(
    process_name: &str,
    process_pid: &str,
    current_pid: &str,
    os: &str,
) -> bool {
    if process_pid == current_pid {
        return false;
    }

    let name = process_name.trim();
    match os {
        // Verified on Windows 11 with Microsoft Store Claude Desktop 1.18286:
        // process name is "claude" and executable path ends in "Claude.exe".
        "windows" => name.eq_ignore_ascii_case("claude") || name.eq_ignore_ascii_case("claude.exe"),
        // Use exact app-process names only; substring matching catches this
        // Warden app and future rebrands containing "Claude".
        "macos" => name == "Claude",
        _ => false,
    }
}

#[cfg(target_os = "windows")]
fn windows_has_visible_window_for_processes(process_ids: &[u32]) -> bool {
    use windows_sys::Win32::{
        Foundation::{BOOL, HWND, LPARAM},
        UI::WindowsAndMessaging::{
            EnumWindows, GetWindowTextLengthW, GetWindowThreadProcessId, IsWindowVisible,
        },
    };

    struct SearchState<'a> {
        process_ids: &'a [u32],
        found: bool,
    }

    unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let state = &mut *(lparam as *mut SearchState<'_>);
        if state.found || IsWindowVisible(hwnd) == 0 || GetWindowTextLengthW(hwnd) <= 0 {
            return 1;
        }

        let mut window_pid = 0;
        GetWindowThreadProcessId(hwnd, &mut window_pid);
        if state.process_ids.contains(&window_pid) {
            state.found = true;
            return 0;
        }

        1
    }

    if process_ids.is_empty() {
        return false;
    }

    let mut state = SearchState {
        process_ids,
        found: false,
    };

    unsafe {
        EnumWindows(
            Some(enum_windows_proc),
            &mut state as *mut SearchState<'_> as LPARAM,
        );
    }

    state.found
}

#[cfg(not(target_os = "windows"))]
fn windows_has_visible_window_for_processes(_process_ids: &[u32]) -> bool {
    false
}

fn calculate_growth_alert(state: &PersistedState) -> GrowthAlert {
    let samples: Vec<_> = state.samples.iter().collect();
    if samples.len() < 2 {
        return GrowthAlert {
            active: false,
            gb_per_hour: 0.0,
            baseline_gb_per_hour: 0.0,
            message: "Not enough samples to calculate growth rate.".to_string(),
        };
    }

    let latest = samples[samples.len() - 1];
    let previous = samples[samples.len() - 2];
    let gb_per_hour = rate_between(previous, latest);
    let baseline_gb_per_hour = if samples.len() >= 4 {
        let mut rates = Vec::new();
        for pair in samples.windows(2).take(samples.len().saturating_sub(2)) {
            rates.push(rate_between(pair[0], pair[1]).max(0.0));
        }
        if rates.is_empty() {
            0.0
        } else {
            rates.iter().sum::<f64>() / rates.len() as f64
        }
    } else {
        0.0
    };

    let threshold = state.settings.growth_alert_gb_per_hour;
    let active = state.settings.growth_alert_enabled
        && gb_per_hour > threshold
        && gb_per_hour > baseline_gb_per_hour.max(0.1) * 2.0;

    GrowthAlert {
        active,
        gb_per_hour,
        baseline_gb_per_hour,
        message: if active {
            format!(
                "Claude cache is growing at {:.1} GB/hour, above the {:.1} GB/hour alert threshold.",
                gb_per_hour, threshold
            )
        } else {
            "Growth rate is within the learned baseline.".to_string()
        },
    }
}

fn rate_between(previous: &SizeSample, latest: &SizeSample) -> f64 {
    let seconds = (latest.captured_at - previous.captured_at)
        .num_seconds()
        .max(1) as f64;
    let delta = latest.total_bytes as f64 - previous.total_bytes as f64;
    bytes_to_gb(delta.max(0.0) as u64) / (seconds / 3600.0)
}

fn bytes_to_gb(bytes: u64) -> f64 {
    bytes as f64 / 1024_f64.powi(3)
}

fn normalize_settings(mut settings: SchedulerSettings) -> SchedulerSettings {
    if NaiveTime::parse_from_str(&settings.schedule_time, "%H:%M").is_err() {
        settings.schedule_time = "02:00".to_string();
    }
    if settings.threshold_gb < 1.0 {
        settings.threshold_gb = 1.0;
    }
    if settings.growth_alert_gb_per_hour < 0.1 {
        settings.growth_alert_gb_per_hour = 0.1;
    }
    settings
}

trait TimeParts {
    fn hour(&self) -> u32;
    fn minute(&self) -> u32;
}

impl TimeParts for NaiveTime {
    fn hour(&self) -> u32 {
        chrono::Timelike::hour(self)
    }

    fn minute(&self) -> u32 {
        chrono::Timelike::minute(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_process_match_excludes_current_pid() {
        assert!(!is_claude_desktop_process_name_for_os(
            "claude", "42", "42", "windows"
        ));
    }

    #[test]
    fn claude_process_match_uses_exact_windows_name() {
        assert!(is_claude_desktop_process_name_for_os(
            "claude", "100", "42", "windows"
        ));
        assert!(is_claude_desktop_process_name_for_os(
            "Claude.exe",
            "100",
            "42",
            "windows"
        ));
        assert!(!is_claude_desktop_process_name_for_os(
            "Claude Cache Warden",
            "100",
            "42",
            "windows"
        ));
        assert!(!is_claude_desktop_process_name_for_os(
            "my-claude-helper",
            "100",
            "42",
            "windows"
        ));
    }

    #[test]
    fn claude_process_match_uses_exact_macos_name() {
        assert!(is_claude_desktop_process_name_for_os(
            "Claude", "100", "42", "macos"
        ));
        assert!(!is_claude_desktop_process_name_for_os(
            "Claude Cache Warden",
            "100",
            "42",
            "macos"
        ));
        assert!(!is_claude_desktop_process_name_for_os(
            "claude", "100", "42", "macos"
        ));
    }

    #[test]
    fn store_package_roots_are_not_default_cleanup_targets() {
        let package_root =
            PathBuf::from(r"C:\Users\Admin\AppData\Local\Packages\Claude_pzs8sxrjxfjjc");
        let roots = windows_store_package_branch_roots(&package_root);

        assert_eq!(
            roots
                .iter()
                .find(|root| is_path_leaf(&root.path, "LocalCache"))
                .map(|root| (root.safety, root.default_cleanup)),
            Some((SafetyLevel::NotRecommended, false))
        );
        assert_eq!(
            roots
                .iter()
                .find(|root| is_path_leaf(&root.path, "TempState"))
                .map(|root| (root.safety, root.default_cleanup)),
            Some((SafetyLevel::Safe, true))
        );
        assert_eq!(
            roots
                .iter()
                .find(|root| is_path_leaf(&root.path, "LocalState"))
                .map(|root| (root.safety, root.default_cleanup)),
            Some((SafetyLevel::NotRecommended, false))
        );
        assert_eq!(
            roots
                .iter()
                .find(|root| is_path_leaf(&root.path, "SystemAppData"))
                .map(|root| (root.safety, root.default_cleanup)),
            Some((SafetyLevel::NotRecommended, false))
        );
    }

    #[test]
    fn conventional_safe_roots_remain_default_cleanup_targets() {
        let root = root(
            PathBuf::from("/Users/example/Library/Application Support/Claude/Cache"),
            "Renderer cache",
        );

        assert_eq!(root.safety, SafetyLevel::Safe);
        assert!(root.default_cleanup);
    }

    #[test]
    fn cleanup_safe_target_rejects_workspace_container_roots() {
        assert!(!is_cleanup_safe_target(&PathBuf::from(
            r"C:\Users\Admin\AppData\Local\Packages\Claude_pzs8sxrjxfjjc\LocalCache"
        )));
        assert!(!is_cleanup_safe_target(&PathBuf::from(
            "/Users/example/Library/Application Support/Claude/vm_bundles"
        )));
        assert!(is_cleanup_safe_target(&PathBuf::from(
            "/Users/example/Library/Application Support/Claude/vm_bundles/warm"
        )));
        assert!(is_cleanup_safe_target(&PathBuf::from(
            r"C:\Users\Admin\AppData\Local\Packages\Claude_pzs8sxrjxfjjc\LocalCache\Roaming\Claude\vm_bundles\claudevm.bundle"
        )));
        assert!(is_cleanup_safe_target(&PathBuf::from(
            "/Users/example/Library/Application Support/Claude/Cache"
        )));
    }

    #[test]
    fn cleanup_safe_target_rejects_claude_state_and_project_paths() {
        let base = PathBuf::from(
            r"C:\Users\Admin\AppData\Local\Packages\Claude_pzs8sxrjxfjjc\LocalCache\Roaming\Claude",
        );

        for relative in [
            "IndexedDB",
            "Local Storage",
            "Session Storage",
            "Network",
            "Partitions",
            "local-agent-mode-sessions",
            "vm_bundles",
            "vm_bundles\\project",
            "claude-code",
            "claude-code-vm",
            "config.json",
            "Preferences",
            "claude_desktop_config.json",
            "git-worktrees.json",
            "config\\Cache",
            "SharedStorage",
            "WebStorage",
        ] {
            let path = base.join(relative);
            assert!(
                !is_cleanup_safe_target(&path),
                "state path must not be cleanup-safe: {}",
                path.display()
            );
        }
    }

    #[test]
    fn cleanup_safe_target_allows_only_rebuildable_cache_leaves() {
        let base = PathBuf::from(
            r"C:\Users\Admin\AppData\Local\Packages\Claude_pzs8sxrjxfjjc\LocalCache\Roaming\Claude",
        );

        for relative in [
            "Cache",
            "Code Cache",
            "GPUCache",
            "DawnGraphiteCache",
            "DawnWebGPUCache",
        ] {
            let path = base.join(relative);
            assert!(
                is_cleanup_safe_target(&path),
                "cache leaf should be cleanup-safe: {}",
                path.display()
            );
        }
    }

    #[test]
    fn scan_selects_safe_cache_leaves_inside_protected_store_root() {
        let root_dir =
            std::env::temp_dir().join(format!("ccw-scan-test-{}-LocalCache", std::process::id()));
        let claude_dir = root_dir.join("Roaming").join("Claude");
        let cache_dir = claude_dir.join("Cache");
        let vm_bundle_dir = claude_dir.join("vm_bundles").join("claudevm.bundle");
        let indexed_db_dir = claude_dir.join("IndexedDB");
        let local_storage_dir = claude_dir.join("Local Storage");
        let _ = fs::remove_dir_all(&root_dir);

        fs::create_dir_all(&cache_dir).unwrap();
        fs::create_dir_all(&vm_bundle_dir).unwrap();
        fs::create_dir_all(&indexed_db_dir).unwrap();
        fs::create_dir_all(&local_storage_dir).unwrap();
        fs::write(cache_dir.join("renderer-cache.bin"), b"cache").unwrap();
        fs::write(vm_bundle_dir.join("rootfs.vhdx"), b"vm-cache").unwrap();
        fs::write(indexed_db_dir.join("chat-state.leveldb"), b"state").unwrap();
        fs::write(local_storage_dir.join("settings.localstorage"), b"state").unwrap();

        let (safety, description) = classify_path(&root_dir);
        let node = scan_path(
            &root_dir,
            "LocalCache".to_string(),
            safety,
            false,
            description,
        )
        .unwrap();
        let mut cleanup_paths = Vec::new();
        collect_default_cleanup_paths(&node, &mut cleanup_paths);

        assert!(cleanup_paths.contains(&cache_dir.to_string_lossy().to_string()));
        assert!(cleanup_paths.contains(&vm_bundle_dir.to_string_lossy().to_string()));
        assert!(!cleanup_paths.contains(&root_dir.to_string_lossy().to_string()));
        assert!(!cleanup_paths.contains(&indexed_db_dir.to_string_lossy().to_string()));
        assert!(!cleanup_paths.contains(&local_storage_dir.to_string_lossy().to_string()));

        let _ = fs::remove_dir_all(&root_dir);
    }

    #[test]
    fn windows_temp_claude_root_is_safe_cleanup_target() {
        let root = root(
            PathBuf::from(r"C:\Users\Admin\AppData\Local\Temp\claude"),
            "Claude temp files",
        );

        assert_eq!(root.safety, SafetyLevel::Safe);
        assert!(root.default_cleanup);
    }

    #[test]
    fn cleanup_removes_nested_contents_without_removing_selected_directory() {
        let root =
            std::env::temp_dir().join(format!("ccw-cleanup-test-{}-nested", std::process::id()));
        let nested = root.join("child").join("nested");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join("top.txt"), b"top").unwrap();
        fs::write(nested.join("cache.bin"), b"cache").unwrap();

        let stats = remove_path_contents_or_file(&root);

        assert!(stats.errors.is_empty(), "{:?}", stats.errors);
        assert!(root.exists());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
        assert!(stats.removed_entries >= 3);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cleanup_vm_bundle_preserves_session_data_file() {
        let root = std::env::temp_dir().join(format!("ccw-vm-bundle-test-{}", std::process::id()));
        let bundle = root.join("vm_bundles").join("claudevm.bundle");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&bundle).unwrap();
        fs::write(bundle.join("rootfs.vhdx"), b"runtime-cache").unwrap();
        fs::write(bundle.join("rootfs.vhdx.zst"), b"runtime-cache").unwrap();
        fs::write(bundle.join("initrd"), b"runtime-cache").unwrap();
        fs::write(bundle.join("sessiondata.vhdx"), b"session-state").unwrap();

        let stats = remove_path_contents_or_file(&bundle);

        assert!(stats.errors.is_empty(), "{:?}", stats.errors);
        assert!(bundle.exists());
        assert!(!bundle.join("rootfs.vhdx").exists());
        assert!(!bundle.join("rootfs.vhdx.zst").exists());
        assert!(!bundle.join("initrd").exists());
        assert!(bundle.join("sessiondata.vhdx").exists());

        let _ = fs::remove_dir_all(&root);
    }
}
