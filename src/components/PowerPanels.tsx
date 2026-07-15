import { Clipboard, ExternalLink, RefreshCw, RotateCcw, Trash2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { formatBytes, formatDate } from "../lib/format";
import {
  getFileTypeBreakdown,
  getLargestItems,
  openInFileManager,
  permanentlyDeleteQuarantineEntry,
  restoreQuarantineEntry,
} from "../lib/tauri";
import type {
  CacheNode,
  FileTypeBreakdownResult,
  LargestItem,
  LargestItemsResult,
  QuarantineEntry,
} from "../types";
import type { Language, translations } from "../i18n";

type Copy = (typeof translations)[Language];

export function QuarantinePanel({
  entries,
  copy,
  busy,
  onRefresh,
  onError,
}: {
  entries: QuarantineEntry[];
  copy: Copy;
  busy: boolean;
  onRefresh: () => Promise<void>;
  onError: (message: string) => void;
}) {
  async function restore(entry: QuarantineEntry) {
    if (!window.confirm(copy.quarantine.restoreConfirm)) return;
    try {
      await restoreQuarantineEntry(entry.cleanup_id);
      await onRefresh();
    } catch (error) {
      onError(errorMessage(error));
    }
  }

  async function remove(entry: QuarantineEntry) {
    if (!window.confirm(copy.quarantine.deleteConfirm)) return;
    try {
      await permanentlyDeleteQuarantineEntry(entry.cleanup_id);
      await onRefresh();
    } catch (error) {
      onError(errorMessage(error));
    }
  }

  return (
    <section className="space-y-4">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div><h2 className="text-xl font-black text-text">{copy.quarantine.title}</h2><p className="mt-1 text-sm text-muted">{copy.quarantine.subtitle}</p></div>
        <button className="secondary-button" type="button" onClick={() => void onRefresh()} disabled={busy}><RefreshCw size={17} />{copy.actions.scan}</button>
      </div>
      {entries.length === 0 ? (
        <div className="surface grid min-h-[260px] place-items-center p-8 text-muted">{copy.quarantine.empty}</div>
      ) : (
        <div className="grid gap-3">
          {entries.map((entry) => (
            <article key={entry.cleanup_id} className="surface p-5">
              <div className="flex flex-col justify-between gap-4 md:flex-row md:items-start">
                <div className="min-w-0">
                  <p className="text-xs font-black uppercase tracking-wide text-accent">{formatBytes(entry.size_bytes)} · {entry.file_count.toLocaleString()} {copy.preview.files}</p>
                  <p className="mt-3 text-xs text-muted">{copy.quarantine.original}</p>
                  <code className="mt-1 block break-all text-sm text-text">{entry.display_original_path}</code>
                  <div className="mt-3 flex flex-wrap gap-3 text-xs text-muted">
                    <span>{copy.quarantine.created}: {formatDate(entry.created_at)}</span>
                    <span>{copy.quarantine.expires}: {entry.expiry_date ? formatDate(entry.expiry_date) : copy.quarantine.never}</span>
                  </div>
                </div>
                <div className="flex shrink-0 flex-wrap gap-2">
                  <button className="secondary-button" type="button" onClick={() => void openInFileManager(entry.quarantine_path)} disabled={busy}><ExternalLink size={16} />{copy.actions.open}</button>
                  <button className="secondary-button" type="button" onClick={() => void restore(entry)} disabled={busy || !entry.restore_eligible}><RotateCcw size={16} />{copy.quarantine.restore}</button>
                  <button className="danger-button" type="button" onClick={() => void remove(entry)} disabled={busy}><Trash2 size={16} />{copy.quarantine.delete}</button>
                </div>
              </div>
            </article>
          ))}
        </div>
      )}
    </section>
  );
}

export function AnalysisPanel({ roots, requestedRoot, copy, onError }: { roots: CacheNode[]; requestedRoot: string; copy: Copy; onError: (message: string) => void }) {
  const [root, setRoot] = useState(requestedRoot || roots.find((node) => node.exists)?.path || "");
  const [largest, setLargest] = useState<LargestItemsResult | null>(null);
  const [breakdown, setBreakdown] = useState<FileTypeBreakdownResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [sort, setSort] = useState<"size" | "name" | "modified" | "safety">("size");

  useEffect(() => {
    if (requestedRoot) setRoot(requestedRoot);
  }, [requestedRoot]);

  async function analyze() {
    if (!root) return;
    setBusy(true);
    try {
      const [items, categories] = await Promise.all([getLargestItems(root, 20), getFileTypeBreakdown(root)]);
      setLargest(items);
      setBreakdown(categories);
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  const files = useMemo(() => sortItems(largest?.files ?? [], sort), [largest, sort]);
  const directories = useMemo(() => sortItems(largest?.directories ?? [], sort), [largest, sort]);
  const rootOptions = useMemo(() => {
    const options = roots.filter((node) => node.exists).map((node) => ({ path: node.path, label: node.label }));
    if (requestedRoot && !options.some((option) => option.path === requestedRoot)) {
      options.push({ path: requestedRoot, label: requestedRoot });
    }
    return options;
  }, [requestedRoot, roots]);

  return (
    <section className="space-y-4">
      <div><h2 className="text-xl font-black text-text">{copy.analysis.title}</h2><p className="mt-1 text-sm text-muted">{copy.analysis.subtitle}</p></div>
      <div className="surface flex flex-col gap-3 p-4 md:flex-row md:items-end">
        <label className="field-label min-w-0 flex-1">{copy.analysis.chooseRoot}
          <select className="field-input" value={root} onChange={(event) => setRoot(event.target.value)}>
            {rootOptions.map((node) => <option key={node.path} value={node.path}>{node.label}</option>)}
          </select>
        </label>
        <button className="primary-button" type="button" onClick={() => void analyze()} disabled={busy || !root}><RefreshCw size={17} />{copy.analysis.load}</button>
      </div>
      {!largest ? <div className="surface grid min-h-[220px] place-items-center p-8 text-muted">{copy.analysis.empty}</div> : (
        <>
          <div className="flex justify-end">
            <select className="field-input w-auto" value={sort} onChange={(event) => setSort(event.target.value as typeof sort)} aria-label={copy.actions.details}>
              <option value="size">{copy.overview.totalCache}</option>
              <option value="name">{copy.overview.detectedPaths}</option>
              <option value="modified">{copy.analysis.modified}</option>
              <option value="safety">{copy.overview.safeDefault}</option>
            </select>
          </div>
          <div className="grid gap-4 xl:grid-cols-2">
            <ItemList title={copy.analysis.largestFiles} items={files} copy={copy} onError={onError} />
            <ItemList title={copy.analysis.largestDirectories} items={directories} copy={copy} onError={onError} />
          </div>
          <section className="surface p-5">
            <h3 className="font-black text-text">{copy.analysis.breakdown}</h3>
            <p className="mt-1 text-xs text-muted">{copy.analysis.bestEffort}</p>
            <div className="mt-4 grid gap-3">
              {breakdown?.categories.map((category) => (
                <div key={category.category}>
                  <div className="flex justify-between gap-3 text-sm"><span className="text-text">{copy.analysis.categories[category.category as keyof typeof copy.analysis.categories] ?? category.category}</span><span className="text-muted">{formatBytes(category.size_bytes)} · {category.percentage.toFixed(1)}%</span></div>
                  <div className="mt-1 h-2 overflow-hidden rounded bg-panel2"><div className="h-full bg-accent" style={{ width: `${Math.max(1, category.percentage)}%` }} /></div>
                </div>
              ))}
            </div>
          </section>
        </>
      )}
    </section>
  );
}

function ItemList({ title, items, copy, onError }: { title: string; items: LargestItem[]; copy: Copy; onError: (message: string) => void }) {
  async function copyPath(path: string) {
    try {
      await navigator.clipboard.writeText(path);
    } catch (error) {
      onError(errorMessage(error));
    }
  }
  return (
    <section className="surface p-5"><h3 className="font-black text-text">{title}</h3><div className="mt-3 grid gap-2">
      {items.map((item) => <div key={item.full_path} className="rounded bg-panel2 p-3">
        <div className="flex items-start justify-between gap-3"><div className="min-w-0"><p className="truncate font-semibold text-text">{item.name}</p><code className="mt-1 block break-all text-[11px] text-muted">{item.display_path}</code></div><strong className="shrink-0 text-accent">{formatBytes(item.size_bytes)}</strong></div>
        <div className="mt-2 flex flex-wrap items-center gap-2 text-xs text-muted"><span>{copy.treemap.safety[item.safety]}</span>{item.modified_at && <span>{copy.analysis.modified}: {formatDate(item.modified_at)}</span>}{item.inaccessible && <span className="text-warn">{copy.analysis.inaccessible}</span>}
          <button className="icon-button ml-auto" type="button" title={copy.actions.copyPath} onClick={() => void copyPath(item.full_path)}><Clipboard size={14} /></button>
          <button className="icon-button" type="button" title={copy.actions.open} onClick={() => void openInFileManager(item.full_path).catch((error) => onError(errorMessage(error)))}><ExternalLink size={14} /></button>
        </div>
      </div>)}
    </div></section>
  );
}

function sortItems(items: LargestItem[], sort: "size" | "name" | "modified" | "safety") {
  return [...items].sort((left, right) => {
    if (sort === "name") return left.name.localeCompare(right.name);
    if (sort === "modified") return (right.modified_at ?? "").localeCompare(left.modified_at ?? "");
    if (sort === "safety") return left.safety.localeCompare(right.safety);
    return right.size_bytes - left.size_bytes;
  });
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : typeof error === "string" ? error : String(error);
}
