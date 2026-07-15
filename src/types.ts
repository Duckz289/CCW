export type SafetyLevel = "Safe" | "Caution" | "NotRecommended";
export type ClaudeActivity = "not_detected" | "background" | "window";
export type CleanupErrorCategory =
  | "permission_denied"
  | "file_locked"
  | "path_not_found"
  | "symlink_rejected"
  | "invalid_target"
  | "protected_target"
  | "read_failed"
  | "delete_failed"
  | "restore_conflict"
  | "move_failed"
  | "unknown";
export type CleanupOutcomeStatus = "fully_cleaned" | "partially_cleaned" | "skipped" | "failed" | "quarantined";
export type ScheduleFrequency = "daily" | "weekly" | "monthly" | "startup";

export interface CacheNode {
  label: string;
  path: string;
  size_bytes: number;
  file_count: number;
  dir_count: number;
  exists: boolean;
  safety: SafetyLevel;
  default_cleanup: boolean;
  description: string;
  children: CacheNode[];
}

export interface ScanResult {
  platform: string;
  scanned_at: string;
  total_bytes: number;
  roots: CacheNode[];
  claude_running: boolean;
  claude_activity: ClaudeActivity;
  warnings: string[];
}

export interface CleanRequest {
  paths: string[];
  allow_when_running: boolean;
  quarantine_caution: boolean;
  trigger: "manual" | "schedule" | "threshold" | "startup" | "disk_space" | "tray";
}

export interface CleanupError {
  category: CleanupErrorCategory;
  path: string;
  message: string;
}

export interface ApprovedPath {
  path: string;
  display_path: string;
  safety: SafetyLevel;
  reason: string;
  estimated_bytes: number;
  estimated_file_count: number;
  estimated_directory_count: number;
  requires_quarantine: boolean;
}

export interface RejectedPath {
  path: string;
  display_path: string;
  reason: string;
  category: CleanupErrorCategory;
}

export interface CleanupPreview {
  requested_paths: string[];
  approved_paths: ApprovedPath[];
  rejected_paths: RejectedPath[];
  estimated_bytes: number;
  estimated_file_count: number;
  estimated_directory_count: number;
  protected_items_detected: boolean;
  claude_activity: ClaudeActivity;
  cleanup_blocked: boolean;
  warnings: string[];
  generated_at: string;
}

export interface PathCleanupOutcome {
  path: string;
  display_path: string;
  status: CleanupOutcomeStatus;
  estimated_bytes: number;
  actual_reclaimed_bytes: number;
  files_removed: number;
  directories_removed: number;
  locked_items: string[];
  errors: CleanupError[];
  skip_reason?: string | null;
  quarantine_cleanup_id: string | null;
}

export interface CleanResult {
  estimated_bytes: number;
  actual_reclaimed_bytes: number;
  files_removed: number;
  directories_removed: number;
  paths_cleaned: string[];
  paths_skipped: string[];
  locked_items: string[];
  errors: CleanupError[];
  outcomes: PathCleanupOutcome[];
  duration_ms: number;
  trigger: string;
  quarantine_used: boolean;
  remaining_bytes: number;
  cleaned_at: string;
}

export interface SchedulerSettings {
  enabled: boolean;
  schedule_enabled: boolean;
  schedule_frequency: ScheduleFrequency;
  schedule_time: string;
  weekly_day: number;
  monthly_day: number;
  schedule_grace_minutes: number;
  threshold_enabled: boolean;
  threshold_gb: number;
  disk_space_enabled: boolean;
  monitored_volume: string;
  minimum_free_gb: number;
  minimum_free_percent: number | null;
  target_free_gb: number;
  cleanup_cooldown_hours: number;
  max_cleanup_bytes: number;
  notification_behavior: string;
  growth_alert_enabled: boolean;
  growth_alert_gb_per_hour: number;
  clean_when_claude_running: boolean;
  launch_at_login: boolean;
  start_minimized: boolean;
  scan_on_startup: boolean;
  startup_cleanup_enabled: boolean;
  startup_cleanup_delay_seconds: number;
  quarantine_retention_days: number;
}

export interface CleanHistoryEntry {
  cleaned_at: string;
  estimated_bytes: number;
  actual_reclaimed_bytes: number;
  remaining_bytes: number;
  duration_ms: number;
  trigger: string;
  quarantine_used: boolean;
  outcomes: PathCleanupOutcome[];
  errors: CleanupError[];
  deleted_paths: string[];
  cleaned_bytes: number;
}

export interface GrowthAlert {
  active: boolean;
  gb_per_hour: number;
  baseline_gb_per_hour: number;
  message: string;
}

export type QuarantineStatus = "quarantined" | "restored" | "deleted" | "failed";

export interface QuarantineEntry {
  cleanup_id: string;
  created_at: string;
  original_path: string;
  display_original_path: string;
  quarantine_path: string;
  size_bytes: number;
  file_count: number;
  status: QuarantineStatus;
  restore_eligible: boolean;
  expiry_date: string | null;
  errors: CleanupError[];
}

export interface QuarantineActionResult {
  cleanup_id: string;
  status: QuarantineStatus;
  errors: CleanupError[];
}

export interface LargestItem {
  name: string;
  display_path: string;
  full_path: string;
  size_bytes: number;
  item_type: "file" | "directory";
  modified_at: string | null;
  safety: SafetyLevel;
  cleanup_eligible: boolean;
  inaccessible: boolean;
}

export interface LargestItemsResult {
  root: string;
  files: LargestItem[];
  directories: LargestItem[];
  warnings: CleanupError[];
  generated_at: string;
}

export interface FileTypeCategory {
  category: string;
  size_bytes: number;
  file_count: number;
  percentage: number;
}

export interface FileTypeBreakdownResult {
  root: string;
  total_bytes: number;
  categories: FileTypeCategory[];
  warnings: CleanupError[];
  generated_at: string;
}

export interface VolumeStatus {
  volume: string;
  available_bytes: number;
  total_bytes: number;
  free_percentage: number;
}

export interface GithubIssue {
  number: number;
  title: string;
  state: string;
  html_url: string;
  updated_at: string;
  labels: { name: string }[];
}
