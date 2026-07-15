import {
  AlertTriangle,
  Clipboard,
  Clock,
  Download,
  ExternalLink,
  FolderSearch,
  RefreshCw,
  Settings,
  Trash2,
  X,
} from "lucide-react";
import type { ReactNode } from "react";
import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import normalFrame from "../action/NORMAL.png";
import { IssueHub } from "./components/IssueHub";
import { Mascot } from "./components/Mascot";
import { Treemap } from "./components/Treemap";
import { CleanupPreviewModal, CleanupReportModal, ExportSuccessModal } from "./components/CleanupModals";
import { AnalysisPanel, QuarantinePanel } from "./components/PowerPanels";
import { cleanCache, clearExpiredQuarantine, evaluateGrowthAlert, exportReport, getClaudeActivity, getCleanHistory, getSchedulerSettings, isTauri, listQuarantineEntries, openExportLocation, openInFileManager, previewCleanup, saveSchedulerSettings, scanCache } from "./lib/tauri";
import { formatBytes, formatDate } from "./lib/format";
import { cleanableDescendants, containsNodePath, defaultSafeSelection, flattenNodes, hasNonZeroSize, toggleExactPath } from "./lib/selection";
import type { CacheNode, ClaudeActivity, CleanHistoryEntry, CleanRequest, CleanResult, CleanupPreview, GrowthAlert, QuarantineEntry, ScanResult, SchedulerSettings } from "./types";
import { languageLabels, localizeDynamicText, translations, type Language } from "./i18n";

type Tab = "overview" | "analysis" | "quarantine" | "history" | "automation" | "issues";
type StatusText = Record<Language, string>;
type Copy = (typeof translations)[Language];
type BlockedSelection = {
  label: string;
  path: string;
  safety: string;
};
type ExportedReport = {
  path: string;
  diagnostic: boolean;
};

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
  const [quarantine, setQuarantine] = useState<QuarantineEntry[]>([]);
  const [preview, setPreview] = useState<CleanupPreview | null>(null);
  const [pendingRequest, setPendingRequest] = useState<CleanRequest | null>(null);
  const [cleanResult, setCleanResult] = useState<CleanResult | null>(null);
  const [exportedReport, setExportedReport] = useState<ExportedReport | null>(null);
  const [openingExportFolder, setOpeningExportFolder] = useState(false);
  const [analysisRoot, setAnalysisRoot] = useState("");
  const [status, setStatus] = useState<StatusText>({ en: translations.en.status.ready, vi: translations.vi.status.ready });
  const [blockedSelection, setBlockedSelection] = useState<BlockedSelection | null>(null);
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
      const [nextScan, nextSettings, nextHistory, nextGrowth, nextQuarantine] = await Promise.all([
        scanCache(),
        getSchedulerSettings(),
        getCleanHistory(),
        evaluateGrowthAlert(),
        listQuarantineEntries(),
      ]);
      setScan(nextScan);
      setSettings(nextSettings);
      setHistory(nextHistory);
      setGrowth(nextGrowth);
      setQuarantine(nextQuarantine);
      setSelectedPaths(defaultSafeSelection(nextScan));
      const scannedAt = formatDate(nextScan.scanned_at);
      setStatusText(translations.en.status.scanned(scannedAt), translations.vi.status.scanned(scannedAt));
    } catch (error) {
      setStatusText(error instanceof Error ? error.message : translations.en.status.scanFailed, error instanceof Error ? error.message : translations.vi.status.scanFailed);
    } finally {
      setBusy(false);
    }
  }

  async function loadInitialState() {
    try {
      const initialSettings = await getSchedulerSettings();
      if (initialSettings.scan_on_startup) {
        await refreshAll();
        return;
      }
      const [nextHistory, nextGrowth, nextQuarantine] = await Promise.all([
        getCleanHistory(),
        evaluateGrowthAlert(),
        listQuarantineEntries(),
      ]);
      setSettings(initialSettings);
      setHistory(nextHistory);
      setGrowth(nextGrowth);
      setQuarantine(nextQuarantine);
    } catch (error) {
      setStatusText(getErrorMessage(error, translations.en.status.scanFailed), getErrorMessage(error, translations.vi.status.scanFailed));
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
    void loadInitialState();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps -- intentional one-time startup load

  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    void (async () => {
      const scanUnlisten = await listen("scan-requested", () => void refreshAll());
      const cleanupUnlisten = await listen<CleanResult>("cleanup-completed", (event) => {
        setCleanResult(event.payload);
        setTab("overview");
        void refreshAll();
      });
      const cleanUnlisten = await listen("safe-cleanup-requested", () => {
        void (async () => {
          const [nextScan, nextSettings] = await Promise.all([scanCache(), getSchedulerSettings()]);
          const paths = Array.from(defaultSafeSelection(nextScan));
          setScan(nextScan);
          setSettings(nextSettings);
          setSelectedPaths(new Set(paths));
          setTab("overview");
          await openCleanupPreview(paths, "tray", nextSettings);
        })();
      });
      if (disposed) {
        scanUnlisten();
        cleanupUnlisten();
        cleanUnlisten();
      } else {
        unlisteners.push(scanUnlisten, cleanupUnlisten, cleanUnlisten);
      }
    })();
    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps -- listeners are registered once and read current backend state

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

  function guardCleanupSelection(node: CacheNode) {
    if (selectedPaths.has(node.path) || node.safety !== "NotRecommended") return true;

    const label = localizeDynamicText(language, node.label);
    const safety = copy.treemap.safety[node.safety];
    setBlockedSelection({ label, path: node.path, safety });
    setStatusText(
      translations.en.status.protectedBlocked,
      translations.vi.status.protectedBlocked,
    );
    return false;
  }

  function toggleNode(node: CacheNode) {
    if (!selectedPaths.has(node.path) && node.safety === "NotRecommended") {
      const descendants = cleanableDescendants(node);
      if (descendants.length > 0) {
        setSelectedPaths((current) => {
          const next = new Set(current);
          const descendantPaths = descendants.map((child) => child.path);
          const allSelected = descendantPaths.every((path) => next.has(path));

          for (const path of Array.from(next)) {
            const selectedNode = nodeByPath.get(path);
            if (selectedNode && (containsNodePath(node, path) || containsNodePath(selectedNode, node.path))) {
              next.delete(path);
            }
          }

          if (!allSelected) {
            for (const path of descendantPaths) next.add(path);
          }

          return next;
        });
        const cleanableBytes = descendants.reduce((sum, child) => sum + child.size_bytes, 0);
        const label = localizeDynamicText(language, node.label);
        setStatusText(
          `Selected ${formatBytes(cleanableBytes)} of safe cache inside "${label}". Protected state folders were left out.`,
          `Đã chọn ${formatBytes(cleanableBytes)} cache an toàn bên trong "${label}". Các thư mục state được giữ lại.`,
        );
        return;
      }
    }

    if (!guardCleanupSelection(node)) return;
    setSelectedPaths((current) => toggleExactPath(current, node, nodeByPath));
  }

  async function openCleanupPreview(paths: string[], trigger: CleanRequest["trigger"], activeSettings = settings) {
    if (paths.length === 0) return;
    const request: CleanRequest = {
      paths,
      allow_when_running: !!activeSettings?.clean_when_claude_running,
      quarantine_caution: false,
      trigger,
    };
    setBusy(true);
    try {
      const nextPreview = await previewCleanup(request);
      setPendingRequest(request);
      setPreview(nextPreview);
      setStatusText(translations.en.status.previewReady, translations.vi.status.previewReady);
    } catch (error) {
      setStatusText(getErrorMessage(error, translations.en.status.previewFailed), getErrorMessage(error, translations.vi.status.previewFailed));
    } finally {
      setBusy(false);
    }
  }

  async function handleClean() {
    if (!scan || selectedPaths.size === 0) return;
    await openCleanupPreview(Array.from(selectedPaths), "manual");
  }

  async function confirmCleanup(quarantineCaution: boolean) {
    if (!pendingRequest) return;
    setBusy(true);
    setCleaning(true);
    try {
      const result = await cleanCache({ ...pendingRequest, quarantine_caution: quarantineCaution });
      setPreview(null);
      setPendingRequest(null);
      setCleanResult(result);
      const cleanedBytes = formatBytes(result.actual_reclaimed_bytes);
      const completed = result.paths_cleaned.length;
      const skipped = result.paths_skipped.length;
      setStatusText(
        result.errors.length > 0 || skipped > 0 ? translations.en.status.cleanedWithIssues(cleanedBytes, completed, result.errors.length, skipped) : translations.en.status.cleaned(cleanedBytes, completed),
        result.errors.length > 0 || skipped > 0 ? translations.vi.status.cleanedWithIssues(cleanedBytes, completed, result.errors.length, skipped) : translations.vi.status.cleaned(cleanedBytes, completed),
      );
      await refreshAll();
    } catch (error) {
      setStatusText(getErrorMessage(error, translations.en.status.cleanupFailed), getErrorMessage(error, translations.vi.status.cleanupFailed));
    } finally {
      setCleaning(false);
      setBusy(false);
    }
  }

  async function handleExport(unsanitized = false) {
    if (unsanitized && !window.confirm(copy.status.unsanitizedWarning)) return;
    try {
      const path = await exportReport(unsanitized);
      setStatusText(translations.en.status.reportExported(path), translations.vi.status.reportExported(path));
      setExportedReport({ path, diagnostic: unsanitized });
    } catch (error) {
      setStatusText(error instanceof Error ? error.message : translations.en.status.exportFailed, error instanceof Error ? error.message : translations.vi.status.exportFailed);
    }
  }

  async function handleOpenExportFolder() {
    if (!exportedReport) return;
    setOpeningExportFolder(true);
    try {
      await openExportLocation(exportedReport.path);
      setStatusText(translations.en.status.exportFolderOpened, translations.vi.status.exportFolderOpened);
    } catch (error) {
      setStatusText(getErrorMessage(error, translations.en.status.exportFolderFailed), getErrorMessage(error, translations.vi.status.exportFolderFailed));
    } finally {
      setOpeningExportFolder(false);
    }
  }

  async function refreshQuarantine() {
    setBusy(true);
    try {
      setQuarantine(await listQuarantineEntries());
    } finally {
      setBusy(false);
    }
  }

  async function clearExpiredEntries() {
    setBusy(true);
    try {
      const cleared = await clearExpiredQuarantine();
      setQuarantine(await listQuarantineEntries());
      setStatusText(translations.en.status.expiredQuarantineCleared(cleared.length), translations.vi.status.expiredQuarantineCleared(cleared.length));
    } catch (error) {
      setStatusText(getErrorMessage(error, translations.en.status.actionFailed), getErrorMessage(error, translations.vi.status.actionFailed));
    } finally {
      setBusy(false);
    }
  }

  async function copyPath(path: string) {
    try {
      await navigator.clipboard.writeText(path);
      setStatusText(translations.en.status.pathCopied, translations.vi.status.pathCopied);
    } catch (error) {
      setStatusText(getErrorMessage(error, translations.en.status.actionFailed), getErrorMessage(error, translations.vi.status.actionFailed));
    }
  }

  async function revealPath(path: string) {
    try {
      await openInFileManager(path);
      setStatusText(translations.en.status.fileManagerOpened, translations.vi.status.fileManagerOpened);
    } catch (error) {
      setStatusText(getErrorMessage(error, translations.en.status.actionFailed), getErrorMessage(error, translations.vi.status.actionFailed));
    }
  }

  async function updateSettings(next: SchedulerSettings) {
    const previous = settings;
    setSettings(next);
    try {
      const saved = await saveSchedulerSettings(next);
      setSettings(saved);
      setStatusText(translations.en.status.settingsSaved, translations.vi.status.settingsSaved);
    } catch (error) {
      setSettings(previous);
      setStatusText(getErrorMessage(error, translations.en.status.actionFailed), getErrorMessage(error, translations.vi.status.actionFailed));
    }
  }

  async function updateAllowWhenRunning(checked: boolean) {
    if (!settings) return;
    if (checked && !window.confirm(copy.automation.allowRunningWarning)) return;
    await updateSettings({ ...settings, clean_when_claude_running: checked });
  }

  const tabs: { id: Tab; label: string; short: string }[] = [
    { id: "overview", label: copy.tabs.overview, short: copy.tabShort.overview },
    { id: "analysis", label: copy.tabs.analysis, short: copy.tabShort.analysis },
    { id: "quarantine", label: copy.tabs.quarantine, short: copy.tabShort.quarantine },
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
                <p className="mt-1 text-sm text-muted" aria-live="polite" role="status">{status[language]}</p>
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
              <button className="secondary-button" type="button" onClick={() => void handleExport()}>
                <Download size={18} />
                {copy.actions.export}
              </button>
              <button className="secondary-button" type="button" onClick={() => void handleExport(true)} title={copy.status.unsanitizedWarning}>
                <AlertTriangle size={18} />
                {copy.actions.exportUnsanitized}
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
                    <div key={node.path} className="path-row">
                      <input className="mt-1 h-4 w-4 accent-accent" type="checkbox" checked={selectedPaths.has(node.path)} onChange={() => toggleNode(node)} aria-label={`${copy.actions.cleanNow}: ${localizeDynamicText(language, node.label)}`} />
                      <span className="min-w-0">
                        <span className="block font-semibold">{localizeDynamicText(language, node.label)}</span>
                        <span className="block break-all text-xs leading-5 text-muted">{node.path}</span>
                      </span>
                      <span className="ml-auto shrink-0 text-sm font-semibold text-text">{formatBytes(node.size_bytes)}</span>
                      <button className="icon-button shrink-0" type="button" title={copy.actions.copyPath} onClick={() => void copyPath(node.path)}><Clipboard size={15} /></button>
                      <button className="icon-button shrink-0" type="button" title={copy.actions.open} onClick={() => void revealPath(node.path)}><ExternalLink size={15} /></button>
                      <button className="secondary-button shrink-0" type="button" onClick={() => { setAnalysisRoot(node.path); setTab("analysis"); }}>{copy.actions.details}</button>
                    </div>
                  ))}
                </div>
              </section>
            </div>
          )}

          {tab === "analysis" && (
            <AnalysisPanel
              roots={visibleRoots}
              requestedRoot={analysisRoot}
              copy={copy}
              onError={(message) => setStatusText(message, localizeDynamicText("vi", message))}
              onNotice={(message) => setStatusText(message, localizeDynamicText("vi", message))}
            />
          )}

          {tab === "quarantine" && (
            <div className="space-y-3">
              <div className="flex justify-end">
                <button className="secondary-button" type="button" onClick={() => void clearExpiredEntries()} disabled={busy}>{copy.quarantine.clearExpired}</button>
              </div>
              <QuarantinePanel
                entries={quarantine}
                copy={copy}
                busy={busy}
                onRefresh={refreshQuarantine}
                onError={(message) => setStatusText(message, localizeDynamicText("vi", message))}
                onNotice={(message) => setStatusText(message, localizeDynamicText("vi", message))}
              />
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
                              <p key={`${error.path}-${error.message}`} className="mt-1 break-words">{error.message}</p>
                            ))}
                          </div>
                        )}
                      </div>
                      <div className="text-left md:text-right">
                        <p className="font-black text-accent">{formatBytes(item.actual_reclaimed_bytes || item.cleaned_bytes)}</p>
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
                <label className="field-label">{copy.automation.frequency}
                  <select className="field-input" value={settings.schedule_frequency} onChange={(event) => updateSettings({ ...settings, schedule_frequency: event.target.value as SchedulerSettings["schedule_frequency"] })}>
                    <option value="daily">{copy.automation.daily}</option>
                    <option value="weekly">{copy.automation.weekly}</option>
                    <option value="monthly">{copy.automation.monthly}</option>
                    <option value="startup">{copy.automation.startup}</option>
                  </select>
                </label>
                {settings.schedule_frequency !== "startup" && <label className="field-label">{copy.automation.cleanupTime}<input className="field-input" type="time" value={settings.schedule_time} onChange={(event) => updateSettings({ ...settings, schedule_time: event.target.value })} /></label>}
                {settings.schedule_frequency === "weekly" && <label className="field-label">{copy.automation.weeklyDay}<input className="field-input" type="number" min="1" max="7" value={settings.weekly_day} onChange={(event) => updateSettings({ ...settings, weekly_day: Number(event.target.value) })} /></label>}
                {settings.schedule_frequency === "monthly" && <label className="field-label">{copy.automation.monthlyDay}<input className="field-input" type="number" min="1" max="31" value={settings.monthly_day} onChange={(event) => updateSettings({ ...settings, monthly_day: Number(event.target.value) })} /></label>}
                <label className="field-label">{copy.automation.graceMinutes}<input className="field-input" type="number" min="1" max="180" value={settings.schedule_grace_minutes} onChange={(event) => updateSettings({ ...settings, schedule_grace_minutes: Number(event.target.value) })} /></label>
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
                <Toggle label={copy.automation.allowCleanupWhileClaudeRunning} checked={settings.clean_when_claude_running} onChange={(checked) => void updateAllowWhenRunning(checked)} />
                <label className="field-label">{copy.automation.quarantineRetention}
                  <select className="field-input" value={settings.quarantine_retention_days} onChange={(event) => updateSettings({ ...settings, quarantine_retention_days: Number(event.target.value) })}>
                    <option value="1">{copy.automation.retention1}</option><option value="7">{copy.automation.retention7}</option><option value="14">{copy.automation.retention14}</option><option value="30">{copy.automation.retention30}</option><option value="-1">{copy.automation.retentionNever}</option>
                  </select>
                </label>
              </SettingPanel>

              <SettingPanel title={copy.automation.diskSpace} icon={<FolderSearch size={20} />}>
                <Toggle label={copy.automation.enableDiskSpace} checked={settings.disk_space_enabled} onChange={(checked) => updateSettings({ ...settings, disk_space_enabled: checked })} />
                <label className="field-label">{copy.automation.monitoredVolume}<input className="field-input" value={settings.monitored_volume} onChange={(event) => updateSettings({ ...settings, monitored_volume: event.target.value })} /></label>
                <label className="field-label">{copy.automation.minimumFreeGb}<input className="field-input" type="number" min="0.5" step="0.5" value={settings.minimum_free_gb} onChange={(event) => updateSettings({ ...settings, minimum_free_gb: Number(event.target.value) })} /></label>
                <label className="field-label">{copy.automation.minimumFreePercent}<input className="field-input" type="number" min="1" max="99" value={settings.minimum_free_percent ?? ""} onChange={(event) => updateSettings({ ...settings, minimum_free_percent: event.target.value ? Number(event.target.value) : null })} /></label>
                <label className="field-label">{copy.automation.targetFreeGb}<input className="field-input" type="number" min="1" value={settings.target_free_gb} onChange={(event) => updateSettings({ ...settings, target_free_gb: Number(event.target.value) })} /></label>
                <label className="field-label">{copy.automation.cooldownHours}<input className="field-input" type="number" min="1" max="168" value={settings.cleanup_cooldown_hours} onChange={(event) => updateSettings({ ...settings, cleanup_cooldown_hours: Number(event.target.value) })} /></label>
                <label className="field-label">{copy.automation.maxCleanupGb}<input className="field-input" type="number" min="0.06" step="0.25" value={settings.max_cleanup_bytes / 1024 ** 3} onChange={(event) => updateSettings({ ...settings, max_cleanup_bytes: Number(event.target.value) * 1024 ** 3 })} /></label>
                <label className="field-label">
                  {copy.automation.notificationBehavior}
                  <select className="field-input" value={settings.notification_behavior} onChange={(event) => updateSettings({ ...settings, notification_behavior: event.target.value })}>
                    <option value="in_app">{copy.automation.notificationInApp}</option>
                    <option value="silent">{copy.automation.notificationSilent}</option>
                  </select>
                </label>
              </SettingPanel>

              <SettingPanel title={copy.automation.startupSettings} icon={<Settings size={20} />}>
                <Toggle label={copy.automation.launchAtLogin} checked={settings.launch_at_login} onChange={(checked) => updateSettings({ ...settings, launch_at_login: checked })} />
                <Toggle label={copy.automation.startMinimized} checked={settings.start_minimized} onChange={(checked) => updateSettings({ ...settings, start_minimized: checked })} />
                <Toggle label={copy.automation.scanOnStartup} checked={settings.scan_on_startup} onChange={(checked) => updateSettings({ ...settings, scan_on_startup: checked })} />
                <Toggle label={copy.automation.safeCleanupOnStartup} checked={settings.startup_cleanup_enabled} onChange={(checked) => updateSettings({ ...settings, startup_cleanup_enabled: checked })} />
                <label className="field-label">{copy.automation.startupDelay}<input className="field-input" type="number" min="5" max="3600" value={settings.startup_cleanup_delay_seconds} onChange={(event) => updateSettings({ ...settings, startup_cleanup_delay_seconds: Number(event.target.value) })} /></label>
              </SettingPanel>
            </div>
          )}

          {tab === "issues" && <IssueHub copy={copy.issues} language={language} />}
        </section>
      </div>
      {blockedSelection && (
        <SafetyDialog
          iconSrc={normalFrame}
          item={blockedSelection}
          copy={copy}
          busy={busy}
          onClose={() => setBlockedSelection(null)}
        />
      )}
      {preview && (
        <CleanupPreviewModal
          preview={preview}
          copy={copy}
          language={language}
          busy={busy}
          onClose={() => { setPreview(null); setPendingRequest(null); }}
          onConfirm={(quarantineCaution) => void confirmCleanup(quarantineCaution)}
        />
      )}
      {cleanResult && <CleanupReportModal result={cleanResult} copy={copy} language={language} onClose={() => setCleanResult(null)} />}
      {exportedReport && (
        <ExportSuccessModal
          path={exportedReport.path}
          diagnostic={exportedReport.diagnostic}
          copy={copy}
          opening={openingExportFolder}
          onClose={() => setExportedReport(null)}
          onOpenFolder={() => void handleOpenExportFolder()}
        />
      )}
    </main>
  );
}

function SafetyDialog({
  iconSrc,
  item,
  copy,
  busy,
  onClose,
}: {
  iconSrc: string;
  item: BlockedSelection;
  copy: Copy;
  busy: boolean;
  onClose: () => void;
}) {
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onClose]);

  return (
    <div
      className="safety-modal-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <section className="safety-modal" role="dialog" aria-modal="true" aria-labelledby="safety-dialog-title">
        <div className="safety-modal-ribbon" />
        <div className="safety-modal-header">
          <div className="safety-modal-icon">
            <img src={iconSrc} alt="" draggable={false} />
          </div>
          <div className="min-w-0">
            <p className="safety-modal-kicker">{copy.safetyDialog.kicker}</p>
            <h2 id="safety-dialog-title" className="safety-modal-title">
              {copy.safetyDialog.title}
            </h2>
          </div>
          <button className="safety-modal-close" type="button" onClick={onClose} aria-label={copy.actions.close}>
            <X size={18} />
          </button>
        </div>

        <div className="safety-modal-body">
          <div className="safety-modal-callout">
            <AlertTriangle size={18} />
            <p>{copy.safetyDialog.description(item.label, item.safety)}</p>
          </div>

          <div>
            <p className="safety-modal-path-label">{copy.safetyDialog.pathLabel}</p>
            <code className="safety-modal-path">{item.path}</code>
          </div>
        </div>

        <div className="safety-modal-footer">
          <button className="secondary-button safety-modal-action" type="button" onClick={onClose} disabled={busy} autoFocus>
            {copy.safetyDialog.understood}
          </button>
        </div>
      </section>
    </div>
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
