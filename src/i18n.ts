import type { SafetyLevel } from "./types";

export type Language = "en" | "vi";

export const languageLabels: Record<Language, string> = {
  en: "EN",
  vi: "VI",
};

export const translations = {
  en: {
    appTitle: "Claude Cache Warden",
    tabs: {
      overview: "Overview",
      history: "History",
      automation: "Automation",
      issues: "Known Issues",
    },
    tabShort: {
      overview: "Map",
      history: "Log",
      automation: "Auto",
      issues: "Live",
    },
    actions: {
      scan: "Scan",
      export: "Export",
      cleanNow: "Clean now",
    },
    status: {
      ready: "Ready",
      scanned: (date: string) => `Scanned ${date}`,
      scanFailed: "Scan failed",
      cleanBlocked: "Claude is running. Enable cleanup while Claude is running in Automation settings to continue.",
      cleanBlockedBackground: "Claude is still running in the background and may be locking cache files. Fully quit Claude from the tray or Task Manager, then scan again.",
      cleaned: (bytes: string, count: number) => `Cleaned ${bytes} from ${count} location(s).`,
      cleanedWithErrors: (bytes: string, count: number, errors: number) => `Cleaned ${bytes} from ${count} location(s), with ${errors} error(s).`,
      cleanupFailed: "Cleanup failed",
      reportExported: (path: string) => `Report exported: ${path}`,
      exportFailed: "Export failed",
    },
    warnings: {
      claudeRunning:
        "Claude Desktop appears to be running. Cleanup is blocked unless you explicitly allow cleanup while Claude is running in Automation settings.",
      claudeBackground:
        "Claude background processes are still running and can lock cache files. Fully quit Claude from the tray or Task Manager before cleaning.",
    },
    overview: {
      kicker: "Swamp scan",
      title: "Cache cleanup with a local warden.",
      body: "Review the cache roots, keep risky folders out of the default selection, and let the warden clean only the paths the backend accepts.",
      totalCache: "Total cache",
      growthRate: "Growth rate",
      learning: "Learning",
      scanning: "Scanning",
      readyToClear: "Ready to clear",
      selectedSummary: (selected: string, total: string) => `${selected} selected path${selected === "1" ? "" : "s"} from ${total} detected entries.`,
      claudeProcess: "Claude process",
      running: "Running",
      background: "Background",
      notDetected: "Not detected",
      wardenState: "Warden state",
      cleaning: "Cleaning",
      alert: "Alert",
      standingBy: "Standing by",
      safeDefault: "Safe default",
      paths: (count: string) => `${count} paths`,
      detectedPaths: "Detected paths",
      detectedPathsBody: "Flat review list for selecting exact cleanup targets.",
    },
    mascot: {
      cleaning: "Cleaning selected cache",
      alert: "Growth spike spotted",
      idle: "Standing guard",
      cleaningDetail: "Selected folders are being removed.",
      alertDetail: "Cache is growing above your threshold.",
      idleDetail: "Safe paths are ready for review.",
      cleaningAria: "Cleaning selected cache folders",
      alertAria: "Cache growth alert",
      idleAria: "Cache watcher standing by",
    },
    treemap: {
      emptyTitle: "Machine is clean",
      emptyBody: "No Claude cache directories were found, or their size is currently negligible.",
      filesFolders: (files: string, folders: string) => `${files} files / ${folders} folders`,
      safety: {
        Safe: "Safe",
        Caution: "Caution",
        NotRecommended: "Not recommended",
      } satisfies Record<SafetyLevel, string>,
    },
    history: {
      empty: "No cleanup history yet.",
      trigger: "Trigger",
      remaining: "Remaining",
      errors: "Errors",
    },
    automation: {
      scheduler: "Scheduler",
      thresholds: "Thresholds",
      enableAutomaticCleanup: "Enable automatic cleanup",
      runOnSchedule: "Run on schedule",
      cleanupTime: "Cleanup time",
      cleanWhenSizeThresholdReached: "Clean when size threshold is reached",
      sizeThresholdGb: "Size threshold in GB",
      alertOnAbnormalGrowthRate: "Alert on abnormal growth rate",
      growthAlertGbHour: "Growth alert in GB/hour",
      allowCleanupWhileClaudeRunning: "Allow cleanup while Claude is running",
    },
    issues: {
      title: "Known Issues",
      subtitle: "Live GitHub status for reported Claude disk usage issues.",
      refresh: "Refresh issues",
      error: "GitHub status could not be fetched. The links below will still open in a browser once loaded.",
      updated: (date: string) => `Updated ${date}`,
    },
  },
  vi: {
    appTitle: "Quản lý cache Claude",
    tabs: {
      overview: "Tổng quan",
      history: "Lịch sử",
      automation: "Tự động",
      issues: "Lỗi đã biết",
    },
    tabShort: {
      overview: "Xem",
      history: "",
      automation: "Auto",
      issues: "Live",
    },
    actions: {
      scan: "Quét",
      export: "Xuất báo cáo",
      cleanNow: "Dọn ngay",
    },
    status: {
      ready: "Sẵn sàng",
      scanned: (date: string) => `Đã quét lúc ${date}`,
      scanFailed: "Quét thất bại",
      cleanBlocked: "Claude đang chạy. Hãy bật tùy chọn cho phép dọn khi Claude đang chạy trong tab Tự động để tiếp tục.",
      cleanBlockedBackground: "Claude vẫn đang chạy nền và có thể đang khóa file cache. Hãy thoát hẳn Claude từ tray hoặc Task Manager rồi quét lại.",
      cleaned: (bytes: string, count: number) => `Đã dọn ${bytes} ở ${count} vị trí.`,
      cleanedWithErrors: (bytes: string, count: number, errors: number) => `Đã dọn ${bytes} ở ${count} vị trí, còn ${errors} lỗi.`,
      cleanupFailed: "Dọn dẹp thất bại",
      reportExported: (path: string) => `Đã xuất báo cáo: ${path}`,
      exportFailed: "Xuất báo cáo thất bại",
    },
    warnings: {
      claudeRunning:
        "Có vẻ Claude Desktop đang chạy. Việc dọn dẹp sẽ bị chặn trừ khi bạn cho phép dọn khi Claude đang chạy trong tab Tự động.",
      claudeBackground:
        "Claude vẫn đang chạy nền và có thể khóa file cache. Hãy thoát hẳn Claude từ tray hoặc Task Manager trước khi dọn.",
    },
    overview: {
      kicker: "Quét nhanh",
      title: "Dọn cache Claude gọn và an toàn.",
      body: "Kiểm tra các thư mục cache, bỏ qua các mục rủi ro trong lựa chọn mặc định, rồi chỉ dọn những đường dẫn được phép.",
      totalCache: "Tổng dung lượng cache",
      growthRate: "Tốc độ tăng",
      learning: "Đang theo dõi",
      scanning: "Đang quét",
      readyToClear: "Sẵn sàng dọn",
      selectedSummary: (selected: string, total: string) => `Đã chọn ${selected} đường dẫn trên tổng ${total} mục phát hiện.`,
      claudeProcess: "Trạng thái Claude",
      running: "Đang chạy",
      background: "Chạy nền",
      notDetected: "Không phát hiện",
      wardenState: "Trạng thái hệ thống",
      cleaning: "Đang dọn",
      alert: "Cảnh báo",
      standingBy: "Sẵn sàng",
      safeDefault: "Chọn mặc định",
      paths: (count: string) => `${count} đường dẫn`,
      detectedPaths: "Các đường dẫn đã phát hiện",
      detectedPathsBody: "Danh sách chi tiết để chọn đúng mục cần dọn.",
    },
    mascot: {
      cleaning: "Đang dọn cache đã chọn",
      alert: "Cache đang tăng nhanh",
      idle: "Đang theo dõi",
      cleaningDetail: "Các thư mục đã chọn đang được xóa.",
      alertDetail: "Tốc độ tăng cache đang vượt ngưỡng bạn đặt.",
      idleDetail: "Các đường dẫn an toàn đã sẵn sàng để kiểm tra.",
      cleaningAria: "Đang dọn các thư mục cache đã chọn",
      alertAria: "Cảnh báo tốc độ tăng cache",
      idleAria: "Hệ thống đang chờ",
    },
    treemap: {
      emptyTitle: "Máy đang sạch",
      emptyBody: "Không tìm thấy thư mục cache Claude, hoặc dung lượng hiện tại không đáng kể.",
      filesFolders: (files: string, folders: string) => `${files} tệp / ${folders} thư mục`,
      safety: {
        Safe: "An toàn",
        Caution: "Cần xem lại",
        NotRecommended: "Giữ nguyên",
      } satisfies Record<SafetyLevel, string>,
    },
    history: {
      empty: "Chưa có lịch sử dọn dẹp.",
      trigger: "Nguồn chạy",
      remaining: "Còn lại",
      errors: "Lỗi",
    },
    automation: {
      scheduler: "Lịch chạy",
      thresholds: "Ngưỡng",
      enableAutomaticCleanup: "Bật dọn dẹp tự động",
      runOnSchedule: "Chạy theo lịch",
      cleanupTime: "Giờ dọn dẹp",
      cleanWhenSizeThresholdReached: "Dọn khi vượt ngưỡng dung lượng",
      sizeThresholdGb: "Ngưỡng dung lượng (GB)",
      alertOnAbnormalGrowthRate: "Cảnh báo khi cache tăng bất thường",
      growthAlertGbHour: "Ngưỡng cảnh báo (GB/giờ)",
      allowCleanupWhileClaudeRunning: "Cho phép dọn khi Claude đang chạy",
    },
    issues: {
      title: "Lỗi đã biết",
      subtitle: "Trạng thái GitHub trực tiếp cho các lỗi Claude chiếm nhiều dung lượng.",
      refresh: "Làm mới",
      error: "Không lấy được trạng thái GitHub. Các liên kết bên dưới vẫn có thể mở trong trình duyệt sau khi tải xong.",
      updated: (date: string) => `Cập nhật ${date}`,
    },
  },
} as const;

const dynamicTextVi: Record<string, string> = {
  "Claude workspace bundles": "Gói workspace Claude",
  "Warm VM bundles": "Gói VM khởi động sẵn",
  "Project bundle cache": "Cache gói dự án",
  "Renderer Cache": "Cache trình hiển thị",
  "Renderer cache": "Cache trình hiển thị",
  "Code Cache": "Cache mã biên dịch",
  "Code cache": "Cache mã biên dịch",
  "Claude Code VM cache": "Cache VM Claude Code",
  "Claude Code cache": "Cache Claude Code",
  "Claude system cache": "Cache hệ thống Claude",
  "Claude roaming data": "Dữ liệu roaming Claude",
  "Claude local cache": "Cache cục bộ Claude",
  "Workspace VM bundles. Active sessions may be using these files.": "Các gói VM của workspace. Phiên đang hoạt động có thể đang dùng các tệp này.",
  "Prebuilt Cowork VM cache that Claude can recreate.": "Cache VM Cowork dựng sẵn mà Claude có thể tạo lại.",
  "Project-scoped bundle data. Review before deleting.": "Dữ liệu gói theo dự án. Nên kiểm tra trước khi xóa.",
  "Application cache that can be rebuilt.": "Cache ứng dụng có thể tạo lại.",
  "Compiled renderer code cache.": "Cache mã trình hiển thị đã biên dịch.",
  "Cache data that Claude can rebuild after cleanup.": "Dữ liệu cache Claude có thể tạo lại sau khi dọn.",
  "May contain settings, session data, or the top-level Claude data folder.": "Có thể chứa cài đặt, dữ liệu phiên hoặc thư mục Claude cấp gốc.",
  "Review this location before deleting because it may be tied to active workspaces.": "Nên kiểm tra vị trí này trước khi xóa vì nó có thể liên quan đến workspace đang hoạt động.",
  "Cache is growing faster than the preview baseline.": "Cache đang tăng nhanh hơn mức tham chiếu của bản xem trước.",
  "Not enough samples to calculate growth rate.": "Chưa đủ mẫu để tính tốc độ tăng.",
  "Growth rate is within the learned baseline.": "Tốc độ tăng vẫn nằm trong mức đã học.",
  manual: "thủ công",
  threshold: "vượt ngưỡng",
  schedule: "theo lịch",
  open: "đang mở",
  closed: "đã đóng",
};

export function localizeDynamicText(language: Language, value: string) {
  if (language === "en") return value;

  const exact = dynamicTextVi[value];
  if (exact) return exact;

  const growthMatch = value.match(/^Claude cache is growing at ([\d.]+) GB\/hour, above the ([\d.]+) GB\/hour alert threshold\.$/);
  if (growthMatch) {
    return `Cache Claude đang tăng ${growthMatch[1]} GB/giờ, vượt ngưỡng cảnh báo ${growthMatch[2]} GB/giờ.`;
  }

  return value;
}
