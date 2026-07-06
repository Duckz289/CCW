import { invoke } from "@tauri-apps/api/core";
import type {
  CleanHistoryEntry,
  CleanRequest,
  CleanResult,
  GrowthAlert,
  ScanResult,
  SchedulerSettings,
} from "../types";

const mockScan: ScanResult = {
  platform: "preview",
  scanned_at: new Date().toISOString(),
  total_bytes: 8.7 * 1024 ** 3,
  claude_running: true,
  warnings: ["Preview data is shown because the app is running outside Tauri."],
  roots: [
    {
      label: "Claude workspace bundles",
      path: "~/Library/Application Support/Claude/vm_bundles",
      size_bytes: 6.4 * 1024 ** 3,
      file_count: 13042,
      dir_count: 832,
      exists: true,
      safety: "Caution",
      default_cleanup: false,
      description: "Workspace VM bundles. Active sessions may be using these files.",
      children: [
        {
          label: "Warm VM bundles",
          path: "~/Library/Application Support/Claude/vm_bundles/warm",
          size_bytes: 4.9 * 1024 ** 3,
          file_count: 9024,
          dir_count: 412,
          exists: true,
          safety: "Safe",
          default_cleanup: false,
          description: "Prebuilt Cowork VM cache that Claude can recreate.",
          children: [],
        },
        {
          label: "Project bundle cache",
          path: "~/Library/Application Support/Claude/vm_bundles/project",
          size_bytes: 1.5 * 1024 ** 3,
          file_count: 4018,
          dir_count: 420,
          exists: true,
          safety: "Caution",
          default_cleanup: false,
          description: "Project-scoped bundle data. Review before deleting.",
          children: [],
        },
      ],
    },
    {
      label: "Renderer Cache",
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
    {
      label: "Code Cache",
      path: "~/Library/Application Support/Claude/Code Cache",
      size_bytes: 1.0 * 1024 ** 3,
      file_count: 2931,
      dir_count: 84,
      exists: true,
      safety: "Safe",
      default_cleanup: true,
      description: "Compiled renderer code cache.",
      children: [],
    },
  ],
};

export async function scanCache(): Promise<ScanResult> {
  if (!("__TAURI_INTERNALS__" in window)) return mockScan;
  return invoke<ScanResult>("scan_cache");
}

export async function cleanCache(request: CleanRequest): Promise<CleanResult> {
  return invoke<CleanResult>("clean_cache", { request });
}

export async function getSchedulerSettings(): Promise<SchedulerSettings> {
  if (!("__TAURI_INTERNALS__" in window)) {
    return {
      enabled: false,
      schedule_enabled: true,
      schedule_time: "02:00",
      threshold_enabled: true,
      threshold_gb: 5,
      growth_alert_enabled: true,
      growth_alert_gb_per_hour: 2,
      clean_when_claude_running: false,
    };
  }
  return invoke<SchedulerSettings>("get_scheduler_settings");
}

export async function saveSchedulerSettings(settings: SchedulerSettings): Promise<SchedulerSettings> {
  if (!("__TAURI_INTERNALS__" in window)) return settings;
  return invoke<SchedulerSettings>("save_scheduler_settings", { settings });
}

export async function getCleanHistory(): Promise<CleanHistoryEntry[]> {
  if (!("__TAURI_INTERNALS__" in window)) return [];
  return invoke<CleanHistoryEntry[]>("get_clean_history");
}

export async function evaluateGrowthAlert(): Promise<GrowthAlert> {
  if (!("__TAURI_INTERNALS__" in window)) {
    return {
      active: true,
      gb_per_hour: 3.4,
      baseline_gb_per_hour: 0.7,
      message: "Cache is growing faster than the preview baseline.",
    };
  }
  return invoke<GrowthAlert>("evaluate_growth_alert");
}

export async function exportReport(): Promise<string> {
  if (!("__TAURI_INTERNALS__" in window)) return "Preview mode does not write reports.";
  return invoke<string>("export_report");
}
