import { invoke } from "@tauri-apps/api/core";
import type {
  CleanHistoryEntry,
  CleanRequest,
  CleanResult,
  ClaudeActivity,
  CleanupPreview,
  FileTypeBreakdownResult,
  GrowthAlert,
  LargestItemsResult,
  QuarantineActionResult,
  QuarantineEntry,
  ScanResult,
  SchedulerSettings,
  VolumeStatus,
} from "../types";

export function isTauri() {
  return "__TAURI_INTERNALS__" in window;
}

export const mockScan: ScanResult = {
  platform: "preview",
  scanned_at: new Date().toISOString(),
  total_bytes: 3.1 * 1024 ** 3,
  claude_running: false,
  claude_activity: "not_detected",
  warnings: ["Preview data is shown because the app is running outside Tauri."],
  roots: [
    {
      label: "Claude workspace bundles",
      path: "~/Library/Application Support/Claude/vm_bundles",
      size_bytes: 1.8 * 1024 ** 3,
      file_count: 3042,
      dir_count: 212,
      exists: true,
      safety: "NotRecommended",
      default_cleanup: false,
      description: "Protected workspace VM bundle container.",
      children: [
        {
          label: "Warm VM bundles",
          path: "~/Library/Application Support/Claude/vm_bundles/warm",
          size_bytes: 1.2 * 1024 ** 3,
          file_count: 2024,
          dir_count: 112,
          exists: true,
          safety: "Safe",
          default_cleanup: true,
          description: "Prebuilt VM cache that Claude can recreate.",
          children: [],
        },
      ],
    },
    {
      label: "Renderer cache",
      path: "~/Library/Application Support/Claude/Cache",
      size_bytes: 1.3 * 1024 ** 3,
      file_count: 4821,
      dir_count: 125,
      exists: true,
      safety: "Safe",
      default_cleanup: true,
      description: "Application cache that can be rebuilt.",
      children: [],
    },
  ],
};

const mockSettings: SchedulerSettings = {
  enabled: false,
  schedule_enabled: false,
  schedule_frequency: "daily",
  schedule_time: "02:00",
  weekly_day: 1,
  monthly_day: 1,
  schedule_grace_minutes: 30,
  threshold_enabled: false,
  threshold_gb: 5,
  disk_space_enabled: false,
  monitored_volume: "",
  minimum_free_gb: 10,
  minimum_free_percent: null,
  target_free_gb: 15,
  cleanup_cooldown_hours: 6,
  max_cleanup_bytes: 2 * 1024 ** 3,
  notification_behavior: "in_app",
  growth_alert_enabled: true,
  growth_alert_gb_per_hour: 2,
  clean_when_claude_running: false,
  launch_at_login: false,
  start_minimized: true,
  scan_on_startup: true,
  startup_cleanup_enabled: false,
  startup_cleanup_delay_seconds: 30,
  quarantine_retention_days: 7,
};

export async function scanCache(): Promise<ScanResult> {
  if (!isTauri()) return { ...mockScan, scanned_at: new Date().toISOString() };
  return invoke<ScanResult>("scan_cache");
}

export async function previewCleanup(request: CleanRequest): Promise<CleanupPreview> {
  if (!isTauri()) {
    const flat = mockScan.roots.flatMap((root) => [root, ...root.children]);
    const approved = flat.filter((node) => request.paths.includes(node.path) && node.safety !== "NotRecommended");
    const rejected = flat.filter((node) => request.paths.includes(node.path) && node.safety === "NotRecommended");
    return {
      requested_paths: request.paths,
      approved_paths: approved.map((node) => ({
        path: node.path,
        display_path: node.path,
        safety: node.safety,
        reason: node.description,
        estimated_bytes: node.size_bytes,
        estimated_file_count: node.file_count,
        estimated_directory_count: node.dir_count,
        requires_quarantine: node.safety === "Caution",
      })),
      rejected_paths: rejected.map((node) => ({
        path: node.path,
        display_path: node.path,
        reason: node.description,
        category: "protected_target",
      })),
      estimated_bytes: approved.reduce((sum, node) => sum + node.size_bytes, 0),
      estimated_file_count: approved.reduce((sum, node) => sum + node.file_count, 0),
      estimated_directory_count: approved.reduce((sum, node) => sum + node.dir_count, 0),
      protected_items_detected: rejected.length > 0,
      claude_activity: mockScan.claude_activity,
      cleanup_blocked: false,
      warnings: [],
      generated_at: new Date().toISOString(),
    };
  }
  return invoke<CleanupPreview>("preview_cleanup", { request });
}

export async function cleanCache(request: CleanRequest): Promise<CleanResult> {
  if (!isTauri()) {
    const preview = await previewCleanup(request);
    return {
      estimated_bytes: preview.estimated_bytes,
      actual_reclaimed_bytes: preview.estimated_bytes,
      files_removed: preview.estimated_file_count,
      directories_removed: preview.estimated_directory_count,
      paths_cleaned: preview.approved_paths.map((path) => path.display_path),
      paths_skipped: preview.rejected_paths.map((path) => path.display_path),
      locked_items: [],
      errors: [],
      outcomes: preview.approved_paths.map((path) => ({
        path: path.path,
        display_path: path.display_path,
        status: path.requires_quarantine ? "quarantined" : "fully_cleaned",
        estimated_bytes: path.estimated_bytes,
        actual_reclaimed_bytes: path.requires_quarantine ? 0 : path.estimated_bytes,
        files_removed: path.requires_quarantine ? 0 : path.estimated_file_count,
        directories_removed: path.requires_quarantine ? 0 : path.estimated_directory_count,
        locked_items: [],
        errors: [],
        quarantine_cleanup_id: path.requires_quarantine ? "preview-entry" : null,
      })),
      duration_ms: 120,
      trigger: request.trigger,
      quarantine_used: preview.approved_paths.some((path) => path.requires_quarantine),
      remaining_bytes: 0,
      cleaned_at: new Date().toISOString(),
    };
  }
  return invoke<CleanResult>("clean_cache", { request });
}

export async function getSchedulerSettings(): Promise<SchedulerSettings> {
  if (!isTauri()) return mockSettings;
  return invoke<SchedulerSettings>("get_scheduler_settings");
}

export async function saveSchedulerSettings(settings: SchedulerSettings): Promise<SchedulerSettings> {
  if (!isTauri()) return settings;
  return invoke<SchedulerSettings>("save_scheduler_settings", { settings });
}

export async function getCleanHistory(): Promise<CleanHistoryEntry[]> {
  if (!isTauri()) return [];
  return invoke<CleanHistoryEntry[]>("get_clean_history");
}

export async function getClaudeActivity(): Promise<ClaudeActivity> {
  if (!isTauri()) return mockScan.claude_activity;
  return invoke<ClaudeActivity>("get_claude_activity");
}

export async function evaluateGrowthAlert(): Promise<GrowthAlert> {
  if (!isTauri()) return { active: false, gb_per_hour: 0.4, baseline_gb_per_hour: 0.3, message: "Growth rate is within the learned baseline." };
  return invoke<GrowthAlert>("evaluate_growth_alert");
}

export async function listQuarantineEntries(): Promise<QuarantineEntry[]> {
  if (!isTauri()) return [];
  return invoke<QuarantineEntry[]>("list_quarantine_entries");
}

export async function restoreQuarantineEntry(cleanupId: string): Promise<QuarantineActionResult> {
  return invoke<QuarantineActionResult>("restore_quarantine_entry", { cleanupId });
}

export async function permanentlyDeleteQuarantineEntry(cleanupId: string): Promise<QuarantineActionResult> {
  return invoke<QuarantineActionResult>("permanently_delete_quarantine_entry", { cleanupId });
}

export async function clearExpiredQuarantine(): Promise<QuarantineActionResult[]> {
  if (!isTauri()) return [];
  return invoke<QuarantineActionResult[]>("clear_expired_quarantine");
}

export async function openInFileManager(path: string): Promise<void> {
  if (!isTauri()) return;
  return invoke<void>("open_in_file_manager", { path });
}

export async function getLargestItems(root: string, limit = 20): Promise<LargestItemsResult> {
  if (!isTauri()) return { root, files: [], directories: [], warnings: [], generated_at: new Date().toISOString() };
  return invoke<LargestItemsResult>("get_largest_items", { root, limit });
}

export async function getFileTypeBreakdown(root: string): Promise<FileTypeBreakdownResult> {
  if (!isTauri()) return { root, total_bytes: 0, categories: [], warnings: [], generated_at: new Date().toISOString() };
  return invoke<FileTypeBreakdownResult>("get_file_type_breakdown", { root });
}

export async function getVolumeStatus(volume: string): Promise<VolumeStatus> {
  return invoke<VolumeStatus>("get_volume_status", { volume });
}

export async function exportReport(unsanitized = false): Promise<string> {
  if (!isTauri()) return "Preview mode does not write reports.";
  return invoke<string>("export_report", { unsanitized });
}

export async function openExportLocation(path: string): Promise<void> {
  if (!isTauri()) return;
  return invoke<void>("open_export_location", { path });
}
