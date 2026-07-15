use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub const DEFAULT_GROWTH_ALERT_GB_PER_HOUR: f64 = 2.0;
pub const DEFAULT_MAX_CLEANUP_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum SafetyLevel {
    Safe,
    Caution,
    NotRecommended,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClaudeActivity {
    NotDetected,
    Background,
    Window,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheNode {
    pub label: String,
    pub path: String,
    pub size_bytes: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub exists: bool,
    pub safety: SafetyLevel,
    pub default_cleanup: bool,
    pub description: String,
    pub children: Vec<CacheNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub platform: String,
    pub scanned_at: String,
    pub total_bytes: u64,
    pub roots: Vec<CacheNode>,
    pub claude_running: bool,
    pub claude_activity: ClaudeActivity,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CleanRequest {
    pub paths: Vec<String>,
    #[serde(default)]
    pub allow_when_running: bool,
    #[serde(default)]
    pub quarantine_caution: bool,
    #[serde(default = "manual_trigger")]
    pub trigger: String,
}

fn manual_trigger() -> String {
    "manual".to_string()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CleanupErrorCategory {
    PermissionDenied,
    FileLocked,
    PathNotFound,
    SymlinkRejected,
    InvalidTarget,
    ProtectedTarget,
    ReadFailed,
    DeleteFailed,
    RestoreConflict,
    MoveFailed,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CleanupError {
    pub category: CleanupErrorCategory,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovedPath {
    pub path: String,
    pub display_path: String,
    pub safety: SafetyLevel,
    pub reason: String,
    pub estimated_bytes: u64,
    pub estimated_file_count: u64,
    pub estimated_directory_count: u64,
    pub requires_quarantine: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectedPath {
    pub path: String,
    pub display_path: String,
    pub reason: String,
    pub category: CleanupErrorCategory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupPreview {
    pub requested_paths: Vec<String>,
    pub approved_paths: Vec<ApprovedPath>,
    pub rejected_paths: Vec<RejectedPath>,
    pub estimated_bytes: u64,
    pub estimated_file_count: u64,
    pub estimated_directory_count: u64,
    pub protected_items_detected: bool,
    pub claude_activity: ClaudeActivity,
    pub cleanup_blocked: bool,
    pub warnings: Vec<String>,
    pub generated_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CleanupOutcomeStatus {
    FullyCleaned,
    PartiallyCleaned,
    Skipped,
    Failed,
    Quarantined,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathCleanupOutcome {
    pub path: String,
    pub display_path: String,
    pub status: CleanupOutcomeStatus,
    pub estimated_bytes: u64,
    pub actual_reclaimed_bytes: u64,
    pub files_removed: u64,
    pub directories_removed: u64,
    pub locked_items: Vec<String>,
    pub errors: Vec<CleanupError>,
    pub quarantine_cleanup_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanResult {
    pub estimated_bytes: u64,
    pub actual_reclaimed_bytes: u64,
    pub files_removed: u64,
    pub directories_removed: u64,
    pub paths_cleaned: Vec<String>,
    pub paths_skipped: Vec<String>,
    pub locked_items: Vec<String>,
    pub errors: Vec<CleanupError>,
    pub outcomes: Vec<PathCleanupOutcome>,
    pub duration_ms: u64,
    pub trigger: String,
    pub quarantine_used: bool,
    pub remaining_bytes: u64,
    pub cleaned_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleFrequency {
    #[default]
    Daily,
    Weekly,
    Monthly,
    Startup,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SchedulerSettings {
    pub enabled: bool,
    pub schedule_enabled: bool,
    pub schedule_frequency: ScheduleFrequency,
    pub schedule_time: String,
    pub weekly_day: u32,
    pub monthly_day: u32,
    pub schedule_grace_minutes: u32,
    pub threshold_enabled: bool,
    pub threshold_gb: f64,
    pub disk_space_enabled: bool,
    pub monitored_volume: String,
    pub minimum_free_gb: f64,
    pub minimum_free_percent: Option<f64>,
    pub target_free_gb: f64,
    pub cleanup_cooldown_hours: u32,
    pub max_cleanup_bytes: u64,
    pub notification_behavior: String,
    pub growth_alert_enabled: bool,
    pub growth_alert_gb_per_hour: f64,
    pub clean_when_claude_running: bool,
    pub launch_at_login: bool,
    pub start_minimized: bool,
    pub scan_on_startup: bool,
    pub startup_cleanup_enabled: bool,
    pub startup_cleanup_delay_seconds: u32,
    pub quarantine_retention_days: i32,
}

impl Default for SchedulerSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            schedule_enabled: false,
            schedule_frequency: ScheduleFrequency::Daily,
            schedule_time: "02:00".to_string(),
            weekly_day: 1,
            monthly_day: 1,
            schedule_grace_minutes: 30,
            threshold_enabled: false,
            threshold_gb: 5.0,
            disk_space_enabled: false,
            monitored_volume: String::new(),
            minimum_free_gb: 10.0,
            minimum_free_percent: None,
            target_free_gb: 15.0,
            cleanup_cooldown_hours: 6,
            max_cleanup_bytes: DEFAULT_MAX_CLEANUP_BYTES,
            notification_behavior: "in_app".to_string(),
            growth_alert_enabled: true,
            growth_alert_gb_per_hour: DEFAULT_GROWTH_ALERT_GB_PER_HOUR,
            clean_when_claude_running: false,
            launch_at_login: false,
            start_minimized: true,
            scan_on_startup: true,
            startup_cleanup_enabled: false,
            startup_cleanup_delay_seconds: 30,
            quarantine_retention_days: 7,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CleanHistoryEntry {
    pub cleaned_at: String,
    pub estimated_bytes: u64,
    pub actual_reclaimed_bytes: u64,
    pub remaining_bytes: u64,
    pub duration_ms: u64,
    pub trigger: String,
    pub quarantine_used: bool,
    pub outcomes: Vec<PathCleanupOutcome>,
    #[serde(default, deserialize_with = "deserialize_history_errors")]
    pub errors: Vec<CleanupError>,
    // Kept for migration from v0.1 state files. New writes contain sanitized paths.
    pub deleted_paths: Vec<String>,
    pub cleaned_bytes: u64,
}

fn deserialize_history_errors<'de, D>(deserializer: D) -> Result<Vec<CleanupError>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum CompatibleError {
        Structured(CleanupError),
        Legacy(String),
    }

    let values = Vec::<CompatibleError>::deserialize(deserializer)?;
    Ok(values
        .into_iter()
        .map(|value| match value {
            CompatibleError::Structured(error) => error,
            CompatibleError::Legacy(message) => CleanupError {
                category: CleanupErrorCategory::Unknown,
                path: String::new(),
                message,
            },
        })
        .collect())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SizeSample {
    pub captured_at: DateTime<Local>,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrowthAlert {
    pub active: bool,
    pub gb_per_hour: f64,
    pub baseline_gb_per_hour: f64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PersistedState {
    pub settings: SchedulerSettings,
    pub history: Vec<CleanHistoryEntry>,
    pub samples: VecDeque<SizeSample>,
    pub last_schedule_occurrence: Option<String>,
    pub last_cleanup_at: Option<DateTime<Local>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QuarantineStatus {
    Quarantined,
    Restored,
    Deleted,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantineEntry {
    pub cleanup_id: String,
    pub created_at: String,
    pub original_path: String,
    pub display_original_path: String,
    pub quarantine_path: String,
    pub size_bytes: u64,
    pub file_count: u64,
    pub status: QuarantineStatus,
    pub restore_eligible: bool,
    pub expiry_date: Option<String>,
    pub errors: Vec<CleanupError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantineActionResult {
    pub cleanup_id: String,
    pub status: QuarantineStatus,
    pub errors: Vec<CleanupError>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LargestItemType {
    File,
    Directory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LargestItem {
    pub name: String,
    pub display_path: String,
    pub full_path: String,
    pub size_bytes: u64,
    pub item_type: LargestItemType,
    pub modified_at: Option<String>,
    pub safety: SafetyLevel,
    pub cleanup_eligible: bool,
    pub inaccessible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LargestItemsResult {
    pub root: String,
    pub files: Vec<LargestItem>,
    pub directories: Vec<LargestItem>,
    pub warnings: Vec<CleanupError>,
    pub generated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTypeCategory {
    pub category: String,
    pub size_bytes: u64,
    pub file_count: u64,
    pub percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTypeBreakdownResult {
    pub root: String,
    pub total_bytes: u64,
    pub categories: Vec<FileTypeCategory>,
    pub warnings: Vec<CleanupError>,
    pub generated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeStatus {
    pub volume: String,
    pub available_bytes: u64,
    pub total_bytes: u64,
    pub free_percentage: f64,
}
