import {
  AlertTriangle,
  Clock,
  Download,
  FolderSearch,
  RefreshCw,
  Settings,
  Trash2,
} from "lucide-react";
import type { ReactNode } from "react";
import { useEffect, useMemo, useState } from "react";
import normalFrame from "../action/NORMAL.png";
import { IssueHub } from "./components/IssueHub";
import { Mascot } from "./components/Mascot";
import { Treemap } from "./components/Treemap";
import { cleanCache, evaluateGrowthAlert, exportReport, getClaudeActivity, getCleanHistory, getSchedulerSettings, saveSchedulerSettings, scanCache } from "./lib/tauri";
import { formatBytes, formatDate } from "./lib/format";
import type { CacheNode, ClaudeActivity, CleanHistoryEntry, GrowthAlert, ScanResult, SchedulerSettings } from "./types";
import { languageLabels, localizeDynamicText, translations, type Language } from "./i18n";

type Tab = "overview" | "history" | "automation" | "issues";
type StatusText = Record<Language, string>;
type Copy = (typeof translations)[Language];

function flattenNodes(nodes: CacheNode[]): CacheNode[] {
  return nodes.flatMap((node) => [node, ...flattenNodes(node.children)]);
}

function hasNonZeroSize(node: CacheNode): boolean {
  return node.exists && node.size_bytes > 0;
}

function defaultSafeSelection(scan: ScanResult | null): Set<string> {
  if (!scan) return new Set();
  return new Set(flattenNodes(scan.roots).filter((node) => node.default_cleanup && hasNonZeroSize(node)).map((node) => node.path));
}

function containsNodePath(parent: CacheNode, path: string): boolean {
  return parent.children.some((child) => child.path === path || containsNodePath(child, path));
}

export default function App() {
  const [language, setLanguage] = useState<Language>(() => {
    const saved = window.localStorage.getItem("ccw-language");
    return saved === "vi" || saved === "en" ? saved : "en";
  });
  const [tab, setTab] = useState<Tab>("overview");
  const [scan, setScan] = useState<ScanResult | null>(null);
  const [selectedPaths, setSelectedPaths] = useState<Set<string>>(new Set());
  const [settings, setSettings] = useState<SchedulerSettings | null>(null);
  const [history, setHistory] = useState<CleanHistoryEntry[]>([]);
  const [growth, setGrowth] = useState<GrowthAlert | null>(null);
  const [status, setStatus] = useState<StatusText>({ en: translations.en.status.ready, vi: translations.vi.status.ready });
  const [busy, setBusy] = useState(false);
  const [cleaning, setCleaning] = useState(false);
  const copy = translations[language];

  function setStatusText(en: string, vi = en) {
    setStatus({ en, vi });
  }

  function changeLanguage(nextLanguage: Language) {
    setLanguage(nextLanguage);
    window.localStorage.setItem("ccw-language", nextLanguage);
  }

  async function refreshAll() {
    setBusy(true);
    try {
      const [nextScan, nextSettings, nextHistory, nextGrowth] = await Promise.all([
        scanCache(),
        getSchedulerSettings(),
        getCleanHistory(),
        evaluateGrowthAlert(),
      ]);
      setScan(nextScan);
      setSettings(nextSettings);
      setHistory(nextHistory);
      setGrowth(nextGrowth);
      setSelectedPaths(defaultSafeSelection(nextScan));
      const scannedAt = formatDate(nextScan.scanned_at);
      setStatusText(translations.en.status.scanned(scannedAt), translations.vi.status.scanned(scannedAt));
    } catch (error) {
      setStatusText(error instanceof Error ? error.message : translations.en.status.scanFailed, error instanceof Error ? error.message : translations.vi.status.scanFailed);
    } finally {
      setBusy(false);
    }
  }

  async function refreshClaudeActivity() {
    try {
      const claudeActivity = await getClaudeActivity();
      setScan((current) => current ? { ...current, claude_activity: claudeActivity, claude_running: claudeActivity === "window" } : current);
    } catch {
      // Process status is advisory; full scans and cleanup still validate in the backend.
    }
  }

  useEffect(() => {
    void refreshAll();
  }, []);

  useEffect(() => {
    const interval = window.setInterval(() => {
      void refreshClaudeActivity();
    }, 5000);
    const handleVisibility = () => {
      if (!document.hidden) void refreshClaudeActivity();
    };
    const handleFocus = () => {
      void refreshClaudeActivity();
    };
    window.addEventListener("focus", handleFocus);
    document.addEventListener("visibilitychange", handleVisibility);
    return () => {
      window.clearInterval(interval);
      window.removeEventListener("focus", handleFocus);
      document.removeEventListener("visibilitychange", handleVisibility);
    };
  }, []);

  useEffect(() => {
    document.documentElement.lang = language;
    document.title = copy.appTitle;
  }, [copy.appTitle, language]);

  const flattenedNodes = useMemo(() => flattenNodes(scan?.roots ?? []), [scan]);
  const visibleNodes = useMemo(() => flattenedNodes.filter(hasNonZeroSize), [flattenedNodes]);
  const visibleRoots = useMemo(() => (scan?.roots ?? []).filter(hasNonZeroSize), [scan]);

  const selectedBytes = useMemo(() => {
    return visibleNodes
      .filter((node) => selectedPaths.has(node.path))
      .reduce((sum, node) => sum + node.size_bytes, 0);
  }, [selectedPaths, visibleNodes]);

  const nodeByPath = useMemo(() => {
    return new Map(visibleNodes.map((node) => [node.path, node]));
  }, [visibleNodes]);

  function confirmRiskySelection(node: CacheNode) {
    if (node.safety === "Safe" || selectedPaths.has(node.path)) return true;
    const label = localizeDynamicText(language, node.label);
    const safety = copy.treemap.safety[node.safety];
    return window.confirm(
      language === "vi"
        ? `Mục "${label}" được đánh dấu ${safety}. Chọn mục này sẽ đưa toàn bộ đường dẫn đó vào danh sách dọn. Bạn muốn tiếp tục?`
        : `"${label}" is marked ${safety}. Selecting it adds the whole path to the cleanup list. Continue?`,
    );
  }

  function toggleNode(node: CacheNode) {
    if (!confirmRiskySelection(node)) return;
    setSelectedPaths((current) => {
      const next = new Set(current);
      if (next.has(node.path)) {
        next.delete(node.path);
      } else {
        // Keep cleanup targets exact: selecting a parent replaces selected descendants,
        // and selecting a child clears any selected ancestor to avoid double counting.
        for (const path of current) {
          const selectedNode = nodeByPath.get(path);
          if (selectedNode && (containsNodePath(node, path) || containsNodePath(selectedNode, node.path))) {
            next.delete(path);
          }
        }
        next.add(node.path);
      }
      return next;
    });
  }

  async function handleClean() {
    if (!scan || selectedPaths.size === 0) return;
    if (claudeBlocksCleanup(scan.claude_activity) && !settings?.clean_when_claude_running) {
      setStatusText(
        cleanupBlockedMessage(translations.en, scan.claude_activity),
        cleanupBlockedMessage(translations.vi, scan.claude_activity),
      );
      return;
    }
    setBusy(true);
    setCleaning(true);
    try {
      const result = await cleanCache({ paths: Array.from(selectedPaths), allow_when_running: !!settings?.clean_when_claude_running });
      const cleanedBytes = formatBytes(result.cleaned_bytes);
      if (result.errors.length > 0) {
        setStatusText(
          translations.en.status.cleanedWithErrors(cleanedBytes, result.deleted_paths.length, result.errors.length),
          translations.vi.status.cleanedWithErrors(cleanedBytes, result.deleted_paths.length, result.errors.length),
        );
      } else {
        setStatusText(translations.en.status.cleaned(cleanedBytes, result.deleted_paths.length), translations.vi.status.cleaned(cleanedBytes, result.deleted_paths.length));
      }
      await refreshAll();
    } catch (error) {
      setStatusText(getErrorMessage(error, translations.en.status.cleanupFailed), getErrorMessage(error, translations.vi.status.cleanupFailed));
    } finally {
      setCleaning(false);
      setBusy(false);
    }
  }

  async function handleExport() {
    try {
      const path = await exportReport();
      setStatusText(translations.en.status.reportExported(path), translations.vi.status.reportExported(path));
    } catch (error) {
      setStatusText(error instanceof Error ? error.message : translations.en.status.exportFailed, error instanceof Error ? error.message : translations.vi.status.exportFailed);
    }
  }

  async function updateSettings(next: SchedulerSettings) {
    setSettings(next);
    const saved = await saveSchedulerSettings(next);
    setSettings(saved);
  }

  const tabs: { id: Tab; label: string; short: string }[] = [
    { id: "overview", label: copy.tabs.overview, short: copy.tabShort.overview },
    { id: "history", label: copy.tabs.history, short: copy.tabShort.history },
    { id: "automation", label: copy.tabs.automation, short: copy.tabShort.automation },
    { id: "issues", label: copy.tabs.issues, short: copy.tabShort.issues },
  ];

  const alertActive = !!growth?.active;

  return (
    <main className="min-h-[100dvh] bg-ink text-text">
      <div className="mx-auto flex min-h-[100dvh] w-full max-w-[1480px] flex-col px-4 py-4 md:px-7 md:py-6">
        <header className="flex flex-col gap-4 rounded-[28px] bg-panel px-4 py-4 shadow-soft md:px-6">
          <div className="flex flex-col justify-between gap-4 lg:flex-row lg:items-center">
            <div className="flex items-center gap-4">
              <div className="grid h-14 w-14 place-items-center rounded-[18px] bg-panel2 shadow-inner">
                <img className="h-10 w-10 object-contain [image-rendering:pixelated]" src={normalFrame} alt="" draggable={false} />
              </div>
              <div>
                <h1 className="text-2xl font-black tracking-normal text-text">{copy.appTitle}</h1>
                <p className="mt-1 text-sm text-muted">{status[language]}</p>
              </div>
            </div>

            <div className="flex flex-wrap gap-2">
              <div className="language-switch" aria-label="Language">
                {(["vi", "en"] as Language[]).map((item) => (
                  <button
                    key={item}
                    className={`language-button ${language === item ? "language-button-active" : ""}`}
                    type="button"
                    onClick={() => changeLanguage(item)}
                  >
                    {languageLabels[item]}
                  </button>
                ))}
              </div>
              <button className="secondary-button" type="button" onClick={refreshAll} disabled={busy}>
                <RefreshCw size={18} />
                {copy.actions.scan}
              </button>
              <button className="secondary-button" type="button" onClick={handleExport}>
                <Download size={18} />
                {copy.actions.export}
              </button>
              <button className="primary-button" type="button" onClick={handleClean} disabled={busy || selectedPaths.size === 0}>
                <Trash2 size={18} />
                {copy.actions.cleanNow}
              </button>
            </div>
          </div>

          <nav className="grid grid-cols-2 gap-2 md:flex">
            {tabs.map((item) => (
              <button key={item.id} className={`nav-button ${tab === item.id ? "nav-button-active" : ""}`} type="button" onClick={() => setTab(item.id)}>
                {item.short && <span className="nav-mark">{item.short}</span>}
                <span>{item.label}</span>
              </button>
            ))}
          </nav>
        </header>

        {scan && claudeBlocksCleanup(scan.claude_activity) && (
          <div className="swamp-alert mt-5 bg-warn/15 text-warn">
            <AlertTriangle className="mt-0.5 shrink-0" size={18} />
            <p className="text-sm">{claudeWarning(copy, scan.claude_activity)}</p>
          </div>
        )}

        {growth?.active && (
          <div className="swamp-alert mt-3 bg-danger/12 text-danger">
            <AlertTriangle className="mt-0.5 shrink-0" size={18} />
            <p className="text-sm">{localizeDynamicText(language, growth.message)}</p>
          </div>
        )}

        {!!scan?.warnings.length && (
          <div className="swamp-alert mt-3 bg-panel2 text-muted">
            <AlertTriangle className="mt-0.5 shrink-0 text-warn" size={18} />
            <div className="space-y-1 text-sm">
              {scan.warnings.map((warning) => (
                <p key={warning}>{localizeDynamicText(language, warning)}</p>
              ))}
            </div>
          </div>
        )}

        <section className="min-w-0 flex-1 py-6">
          {tab === "overview" && (
            <div className="space-y-6">
              <section className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_minmax(320px,440px)_minmax(0,1fr)]">
                <div className="surface space-y-5 p-5 md:p-6">
                  <div>
                    <p className="text-sm font-semibold text-accent">{copy.overview.kicker}</p>
                    <h2 className="mt-2 max-w-[12ch] text-4xl font-black leading-tight text-text md:text-5xl">{copy.overview.title}</h2>
                  </div>
                  <p className="max-w-[52ch] text-sm leading-6 text-muted">
                    {copy.overview.body}
                  </p>
                  <div className="grid gap-3 sm:grid-cols-2">
                    <Metric label={copy.overview.totalCache} value={scan ? formatBytes(scan.total_bytes) : copy.overview.scanning} />
                    <Metric label={copy.overview.growthRate} value={growth ? `${growth.gb_per_hour.toFixed(1)} GB/hr` : copy.overview.learning} />
                  </div>
                </div>

                <Mascot alertActive={alertActive} cleaning={cleaning} copy={copy.mascot} />

                <div className="surface surface-pond p-5 md:p-6">
                  <p className="text-sm font-semibold text-accent">{copy.overview.readyToClear}</p>
                  <p className="mt-3 text-5xl font-black leading-none text-text">{formatBytes(selectedBytes)}</p>
                  <p className="mt-3 text-sm leading-6 text-muted">
                    {copy.overview.selectedSummary(selectedPaths.size.toLocaleString(), visibleNodes.length.toLocaleString())}
                  </p>
                  <div className="mt-6 grid gap-3">
                    <StatusLine label={copy.overview.claudeProcess} value={claudeActivityLabel(copy, scan?.claude_activity)} active={scan ? claudeBlocksCleanup(scan.claude_activity) : false} />
                    <StatusLine label={copy.overview.wardenState} value={cleaning ? copy.overview.cleaning : alertActive ? copy.overview.alert : copy.overview.standingBy} active={alertActive || cleaning} />
                    <StatusLine label={copy.overview.safeDefault} value={copy.overview.paths(defaultSafeSelection(scan).size.toLocaleString())} active={false} />
                  </div>
                </div>
              </section>

              <Treemap nodes={visibleRoots} selectedPaths={selectedPaths} onToggleNode={toggleNode} copy={copy.treemap} language={language} />

              <section className="surface p-4 md:p-5">
                <div className="flex flex-col justify-between gap-2 md:flex-row md:items-end">
                  <div>
                    <h3 className="text-xl font-black text-text">{copy.overview.detectedPaths}</h3>
                    <p className="mt-1 text-sm text-muted">{copy.overview.detectedPathsBody}</p>
                  </div>
                  <FolderSearch className="hidden text-accent md:block" size={24} />
                </div>
                <div className="mt-4 grid gap-2">
                  {visibleNodes.map((node) => (
                    <label key={node.path} className="path-row">
                      <input className="mt-1 h-4 w-4 accent-accent" type="checkbox" checked={selectedPaths.has(node.path)} onChange={() => toggleNode(node)} />
                      <span className="min-w-0">
                        <span className="block font-semibold">{localizeDynamicText(language, node.label)}</span>
                        <span className="block break-all text-xs leading-5 text-muted">{node.path}</span>
                      </span>
                      <span className="ml-auto shrink-0 text-sm font-semibold text-text">{formatBytes(node.size_bytes)}</span>
                    </label>
                  ))}
                </div>
              </section>
            </div>
          )}

          {tab === "history" && (
            <div className="surface overflow-hidden">
              {history.length === 0 ? (
                <div className="grid min-h-[280px] place-items-center p-8 text-center text-muted">{copy.history.empty}</div>
              ) : (
                <div className="divide-y divide-line/70">
                  {history.map((item) => (
                    <div key={`${item.cleaned_at}-${item.trigger}`} className="grid gap-3 p-5 md:grid-cols-[1fr_auto]">
                      <div>
                        <p className="font-black text-text">{formatDate(item.cleaned_at)}</p>
                        <p className="mt-1 text-sm text-muted">{copy.history.trigger}: {localizeDynamicText(language, item.trigger)}</p>
                        {item.errors.length > 0 && (
                          <div className="mt-3 rounded border border-warn/30 bg-warn/10 p-3 text-xs text-warn">
                            <p className="font-black">{copy.history.errors}</p>
                            {item.errors.slice(0, 3).map((error) => (
                              <p key={error} className="mt-1 break-words">{error}</p>
                            ))}
                          </div>
                        )}
                      </div>
                      <div className="text-left md:text-right">
                        <p className="font-black text-accent">{formatBytes(item.cleaned_bytes)}</p>
                        <p className="mt-1 text-sm text-muted">{copy.history.remaining} {formatBytes(item.remaining_bytes)}</p>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}

          {tab === "automation" && settings && (
            <div className="grid gap-4 xl:grid-cols-2">
              <SettingPanel title={copy.automation.scheduler} icon={<Clock size={20} />}>
                <Toggle label={copy.automation.enableAutomaticCleanup} checked={settings.enabled} onChange={(checked) => updateSettings({ ...settings, enabled: checked })} />
                <Toggle label={copy.automation.runOnSchedule} checked={settings.schedule_enabled} onChange={(checked) => updateSettings({ ...settings, schedule_enabled: checked })} />
                <label className="field-label">
                  {copy.automation.cleanupTime}
                  <input className="field-input" type="time" value={settings.schedule_time} onChange={(event) => updateSettings({ ...settings, schedule_time: event.target.value })} />
                </label>
              </SettingPanel>

              <SettingPanel title={copy.automation.thresholds} icon={<Settings size={20} />}>
                <Toggle label={copy.automation.cleanWhenSizeThresholdReached} checked={settings.threshold_enabled} onChange={(checked) => updateSettings({ ...settings, threshold_enabled: checked })} />
                <label className="field-label">
                  {copy.automation.sizeThresholdGb}
                  <input className="field-input" type="number" min="1" value={settings.threshold_gb} onChange={(event) => updateSettings({ ...settings, threshold_gb: Number(event.target.value) })} />
                </label>
                <Toggle label={copy.automation.alertOnAbnormalGrowthRate} checked={settings.growth_alert_enabled} onChange={(checked) => updateSettings({ ...settings, growth_alert_enabled: checked })} />
                <label className="field-label">
                  {copy.automation.growthAlertGbHour}
                  <input className="field-input" type="number" min="0.1" step="0.1" value={settings.growth_alert_gb_per_hour} onChange={(event) => updateSettings({ ...settings, growth_alert_gb_per_hour: Number(event.target.value) })} />
                </label>
                <Toggle label={copy.automation.allowCleanupWhileClaudeRunning} checked={settings.clean_when_claude_running} onChange={(checked) => updateSettings({ ...settings, clean_when_claude_running: checked })} />
              </SettingPanel>
            </div>
          )}

          {tab === "issues" && <IssueHub copy={copy.issues} language={language} />}
        </section>
      </div>
    </main>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="metric-tile">
      <p className="text-xs font-semibold uppercase tracking-wide text-muted">{label}</p>
      <p className="mt-2 text-2xl font-black text-text">{value}</p>
    </div>
  );
}

function StatusLine({ label, value, active }: { label: string; value: string; active: boolean }) {
  return (
    <div className="flex items-center justify-between gap-4 bg-panel/70 px-3 py-2">
      <span className="text-sm text-muted">{label}</span>
      <span className={`text-sm font-black ${active ? "text-danger" : "text-accent"}`}>{value}</span>
    </div>
  );
}

function SettingPanel({ title, icon, children }: { title: string; icon: ReactNode; children: ReactNode }) {
  return (
    <section className="surface space-y-4 p-5">
      <div className="flex items-center gap-3 text-accent">
        {icon}
        <h3 className="font-black text-text">{title}</h3>
      </div>
      {children}
    </section>
  );
}

function Toggle({ label, checked, onChange }: { label: string; checked: boolean; onChange: (checked: boolean) => void }) {
  return (
    <label className="flat-control">
      <span className="text-sm font-medium text-text">{label}</span>
      <input className="h-5 w-5 accent-accent" type="checkbox" checked={checked} onChange={(event) => onChange(event.target.checked)} />
    </label>
  );
}

function getErrorMessage(error: unknown, fallback: string) {
  const message = error instanceof Error ? error.message : typeof error === "string" ? error : "";
  if (message.trim().length > 0) return compactStatusMessage(message);
  return fallback;
}

function compactStatusMessage(message: string) {
  const normalized = message.replace(/\s+/g, " ").trim();
  return normalized.length > 240 ? `${normalized.slice(0, 237)}...` : normalized;
}

function claudeBlocksCleanup(activity: ClaudeActivity) {
  return activity !== "not_detected";
}

function claudeActivityLabel(copy: Copy, activity?: ClaudeActivity) {
  if (activity === "window") return copy.overview.running;
  if (activity === "background") return copy.overview.background;
  return copy.overview.notDetected;
}

function claudeWarning(copy: Copy, activity: ClaudeActivity) {
  if (activity === "background") return copy.warnings.claudeBackground;
  return copy.warnings.claudeRunning;
}

function cleanupBlockedMessage(copy: Copy, activity: ClaudeActivity) {
  if (activity === "background") return copy.status.cleanBlockedBackground;
  return copy.status.cleanBlocked;
}
