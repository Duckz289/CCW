import { AlertTriangle, CheckCircle2, FolderOpen, ShieldAlert, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { formatBytes, formatDate } from "../lib/format";
import type { CleanResult, CleanupPreview } from "../types";
import { localizeDynamicText, type Language, type translations } from "../i18n";

type Copy = (typeof translations)[Language];

function useDialog(onClose: () => void) {
  const closeRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    closeRef.current?.focus();
    const handler = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onClose]);
  return closeRef;
}

export function CleanupPreviewModal({
  preview,
  copy,
  language,
  busy,
  onClose,
  onConfirm,
}: {
  preview: CleanupPreview;
  copy: Copy;
  language: Language;
  busy: boolean;
  onClose: () => void;
  onConfirm: (quarantineCaution: boolean) => void;
}) {
  const closeRef = useDialog(onClose);
  const hasCaution = preview.approved_paths.some((path) => path.requires_quarantine);
  const [quarantineCaution, setQuarantineCaution] = useState(false);
  const confirmDisabled =
    busy ||
    preview.cleanup_blocked ||
    preview.approved_paths.length === 0 ||
    (hasCaution && !quarantineCaution);

  return (
    <div className="safety-modal-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <section className="phase-modal" role="dialog" aria-modal="true" aria-labelledby="preview-title">
        <header className="phase-modal-header">
          <div>
            <p className="safety-modal-kicker">{copy.overview.kicker}</p>
            <h2 id="preview-title" className="text-2xl font-black text-text">{copy.preview.title}</h2>
            <p className="mt-1 text-sm text-muted">{copy.preview.subtitle}</p>
          </div>
          <button ref={closeRef} className="safety-modal-close" type="button" onClick={onClose} aria-label={copy.actions.close}>
            <X size={18} />
          </button>
        </header>

        <div className="phase-modal-scroll space-y-4">
          <div className="grid gap-3 sm:grid-cols-4">
            <PreviewMetric label={copy.preview.estimated} value={formatBytes(preview.estimated_bytes)} />
            <PreviewMetric label={copy.preview.files} value={preview.estimated_file_count.toLocaleString()} />
            <PreviewMetric label={copy.preview.directories} value={preview.estimated_directory_count.toLocaleString()} />
            <PreviewMetric label={copy.preview.claudeState} value={copy.preview.activity[preview.claude_activity]} />
          </div>

          {preview.cleanup_blocked && <Notice danger text={copy.preview.blocked} />}
          {preview.approved_paths.length === 0 && <Notice danger text={copy.preview.noApproved} />}

          <section className="modal-list-section">
            <h3 className="font-black text-accent">{copy.preview.approved}</h3>
            <div className="mt-2 grid gap-2">
              {preview.approved_paths.map((path) => (
                <div key={path.path} className="rounded bg-panel2 p-3">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <code className="break-all text-xs text-text">{path.display_path}</code>
                    <span className={path.requires_quarantine ? "badge-warn" : "badge-safe"}>{copy.treemap.safety[path.safety]}</span>
                  </div>
                  <p className="mt-2 text-xs text-muted">{localizeDynamicText(language, path.reason)}</p>
                </div>
              ))}
            </div>
          </section>

          {preview.rejected_paths.length > 0 && (
            <section className="modal-list-section border-danger/30">
              <h3 className="font-black text-danger">{copy.preview.rejected}</h3>
              <div className="mt-2 grid gap-2">
                {preview.rejected_paths.map((path) => (
                  <div key={`${path.path}-${path.category}`} className="rounded bg-danger/8 p-3">
                    <code className="break-all text-xs text-text">{path.display_path}</code>
                    <p className="mt-2 text-xs text-danger">{localizeDynamicText(language, path.reason)}</p>
                  </div>
                ))}
              </div>
            </section>
          )}

          {preview.warnings.length > 0 && (
            <section className="modal-list-section">
              <h3 className="font-black text-warn">{copy.preview.warnings}</h3>
              {preview.warnings.map((warning) => <p key={warning} className="mt-2 text-xs text-muted">{localizeDynamicText(language, warning)}</p>)}
            </section>
          )}

          {hasCaution && (
            <label className="flat-control items-start border border-warn/30 bg-warn/8">
              <input
                className="mt-1 h-5 w-5 accent-accent"
                type="checkbox"
                checked={quarantineCaution}
                onChange={(event) => setQuarantineCaution(event.target.checked)}
              />
              <span>
                <span className="block font-black text-warn">{copy.preview.cautionTitle}</span>
                <span className="mt-1 block text-xs text-muted">{copy.preview.cautionBody}</span>
                <span className="mt-2 block text-sm text-text">{copy.preview.enableQuarantine}</span>
              </span>
            </label>
          )}
        </div>

        <footer className="phase-modal-footer">
          <button className="secondary-button" type="button" onClick={onClose} disabled={busy}>{copy.actions.cancel}</button>
          <button className="danger-button" type="button" onClick={() => onConfirm(quarantineCaution)} disabled={confirmDisabled}>
            <ShieldAlert size={18} />
            {copy.actions.confirm}
          </button>
        </footer>
      </section>
    </div>
  );
}

export function CleanupReportModal({ result, copy, language, onClose }: { result: CleanResult; copy: Copy; language: Language; onClose: () => void }) {
  const closeRef = useDialog(onClose);
  return (
    <div className="safety-modal-backdrop" role="presentation">
      <section className="phase-modal" role="dialog" aria-modal="true" aria-labelledby="report-title">
        <header className="phase-modal-header">
          <div className="flex items-center gap-3">
            <CheckCircle2 className="text-accent" size={26} />
            <h2 id="report-title" className="text-2xl font-black text-text">{copy.report.title}</h2>
          </div>
          <button ref={closeRef} className="safety-modal-close" type="button" onClick={onClose} aria-label={copy.actions.close}><X size={18} /></button>
        </header>
        <div className="phase-modal-scroll space-y-4">
          <div className="grid gap-3 sm:grid-cols-3">
            <PreviewMetric label={copy.report.estimated} value={formatBytes(result.estimated_bytes)} />
            <PreviewMetric label={copy.report.actual} value={formatBytes(result.actual_reclaimed_bytes)} />
            <PreviewMetric label={copy.report.remaining} value={formatBytes(result.remaining_bytes)} />
            <PreviewMetric label={copy.report.files} value={result.files_removed.toLocaleString()} />
            <PreviewMetric label={copy.report.directories} value={result.directories_removed.toLocaleString()} />
            <PreviewMetric label={copy.report.duration} value={`${result.duration_ms.toLocaleString()} ms`} />
          </div>
          <div className="grid gap-2 sm:grid-cols-2">
            <p className="rounded bg-panel2 p-3 text-sm text-muted">{copy.report.trigger}: <strong className="text-text">{result.trigger}</strong></p>
            <p className="rounded bg-panel2 p-3 text-sm text-muted">{copy.report.quarantine}: <strong className="text-text">{result.quarantine_used ? copy.report.yes : copy.report.no}</strong></p>
          </div>
          <section className="modal-list-section">
            <h3 className="font-black text-text">{copy.report.outcomes}</h3>
            <div className="mt-2 grid gap-2">
              {result.outcomes.map((outcome) => (
                <div key={`${outcome.path}-${outcome.status}`} className="rounded bg-panel2 p-3">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <code className="break-all text-xs text-text">{outcome.display_path}</code>
                    <span className={outcome.status === "failed" ? "badge-danger" : outcome.status === "partially_cleaned" ? "badge-warn" : "badge-safe"}>
                      {copy.report.status[outcome.status]}
                    </span>
                  </div>
                  <p className="mt-2 text-xs text-muted">{formatBytes(outcome.actual_reclaimed_bytes)} · {outcome.files_removed.toLocaleString()} {copy.preview.files}</p>
                  {outcome.skip_reason && <p className="mt-2 rounded bg-warn/10 p-2 text-xs text-warn">{localizeDynamicText(language, outcome.skip_reason)}</p>}
                  {outcome.errors.map((error) => <p key={`${error.path}-${error.message}`} className="mt-1 text-xs text-danger">{localizeDynamicText(language, error.message)}</p>)}
                </div>
              ))}
            </div>
          </section>
          {result.locked_items.length > 0 && (
            <section className="modal-list-section border-warn/30">
              <div className="flex items-center gap-2 font-black text-warn"><AlertTriangle size={18} />{copy.report.locked}</div>
              {result.locked_items.map((path) => <code key={path} className="mt-2 block break-all text-xs text-text">{path}</code>)}
              <p className="mt-3 text-xs text-muted">{copy.report.lockedAdvice}</p>
            </section>
          )}
          <p className="text-xs text-muted">{formatDate(result.cleaned_at)}</p>
        </div>
        <footer className="phase-modal-footer"><button className="primary-button" type="button" onClick={onClose}>{copy.actions.close}</button></footer>
      </section>
    </div>
  );
}

export function ExportSuccessModal({
  path,
  diagnostic,
  copy,
  onClose,
  onOpenFolder,
  opening,
}: {
  path: string;
  diagnostic: boolean;
  copy: Copy;
  onClose: () => void;
  onOpenFolder: () => void;
  opening: boolean;
}) {
  const closeRef = useDialog(onClose);
  return (
    <div className="safety-modal-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <section className="phase-modal max-w-[640px]" role="dialog" aria-modal="true" aria-labelledby="export-success-title">
        <header className="phase-modal-header">
          <div className="flex items-center gap-3">
            <CheckCircle2 className="text-accent" size={26} />
            <div>
              <p className="safety-modal-kicker">{copy.appTitle}</p>
              <h2 id="export-success-title" className="text-2xl font-black text-text">{diagnostic ? copy.exportModal.diagnosticTitle : copy.exportModal.title}</h2>
            </div>
          </div>
          <button ref={closeRef} className="safety-modal-close" type="button" onClick={onClose} aria-label={copy.actions.close}>
            <X size={18} />
          </button>
        </header>
        <div className="phase-modal-scroll space-y-4">
          <p className="text-sm leading-6 text-muted">{copy.exportModal.body}</p>
          <section className="modal-list-section">
            <p className="text-xs font-black uppercase tracking-wide text-muted">{copy.exportModal.file}</p>
            <code className="mt-2 block break-all rounded bg-ink px-3 py-3 text-xs text-text">{path}</code>
          </section>
        </div>
        <footer className="phase-modal-footer">
          <button className="secondary-button" type="button" onClick={onClose} disabled={opening}>{copy.actions.close}</button>
          <button className="primary-button" type="button" onClick={onOpenFolder} disabled={opening}>
            <FolderOpen size={18} />
            {copy.exportModal.openFolder}
          </button>
        </footer>
      </section>
    </div>
  );
}

function PreviewMetric({ label, value }: { label: string; value: string }) {
  return <div className="metric-tile"><p className="text-xs font-semibold text-muted">{label}</p><p className="mt-2 text-xl font-black text-text">{value}</p></div>;
}

function Notice({ danger, text }: { danger?: boolean; text: string }) {
  return <div className={`swamp-alert ${danger ? "bg-danger/12 text-danger" : "bg-warn/12 text-warn"}`}><AlertTriangle size={18} /><p className="text-sm">{text}</p></div>;
}
