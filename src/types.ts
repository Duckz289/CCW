export type SafetyLevel = "Safe" | "Caution" | "NotRecommended";

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
  warnings: string[];
}

export interface CleanRequest {
  paths: string[];
  allow_when_running: boolean;
}

export interface CleanResult {
  cleaned_bytes: number;
  deleted_paths: string[];
  skipped_paths: string[];
  errors: string[];
  remaining_bytes: number;
  cleaned_at: string;
}

export interface SchedulerSettings {
  enabled: boolean;
  schedule_enabled: boolean;
  schedule_time: string;
  threshold_enabled: boolean;
  threshold_gb: number;
  growth_alert_enabled: boolean;
  growth_alert_gb_per_hour: number;
  clean_when_claude_running: boolean;
}

export interface CleanHistoryEntry {
  cleaned_at: string;
  cleaned_bytes: number;
  remaining_bytes: number;
  trigger: string;
  deleted_paths: string[];
  errors: string[];
}

export interface GrowthAlert {
  active: boolean;
  gb_per_hour: number;
  baseline_gb_per_hour: number;
  message: string;
}

export interface GithubIssue {
  number: number;
  title: string;
  state: string;
  html_url: string;
  updated_at: string;
  labels: { name: string }[];
}
