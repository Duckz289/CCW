import { Check, ChevronDown, Plus } from "lucide-react";
import { useEffect, useState } from "react";
import type { CacheNode, SafetyLevel } from "../types";
import { formatBytes } from "../lib/format";
import { localizeDynamicText, type Language, type translations } from "../i18n";

interface TreemapProps {
  nodes: CacheNode[];
  selectedPaths: Set<string>;
  onToggleNode: (node: CacheNode) => void;
  copy: (typeof translations)[Language]["treemap"];
  language: Language;
}

const safetyClass: Record<SafetyLevel, string> = {
  Safe: "bg-accent/12 text-accent",
  Caution: "bg-warn/14 text-warn",
  NotRecommended: "bg-danger/12 text-danger",
};

function flattenNodeChildren(node: CacheNode): CacheNode[] {
  return node.children.flatMap((child) => [child, ...flattenNodeChildren(child)]);
}

export function Treemap({ nodes, selectedPaths, onToggleNode, copy, language }: TreemapProps) {
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set());
  const total = nodes.reduce((sum, node) => sum + node.size_bytes, 0);

  useEffect(() => {
    setExpandedPaths(new Set(nodes.filter((node) => node.children.some((child) => child.size_bytes > 0)).map((node) => node.path)));
  }, [nodes]);

  if (total === 0) {
    return (
      <div className="grid min-h-[360px] place-items-center rounded-[24px] bg-panel2/70 p-8 text-center shadow-soft">
        <div>
          <p className="text-lg font-semibold text-text">{copy.emptyTitle}</p>
          <p className="mt-2 max-w-md text-sm text-muted">{copy.emptyBody}</p>
        </div>
      </div>
    );
  }

  function toggleExpanded(path: string) {
    setExpandedPaths((current) => {
      const next = new Set(current);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }

  return (
    <div className="grid auto-rows-[minmax(132px,auto)] grid-cols-1 gap-3 md:grid-cols-6">
      {nodes.map((node) => {
        const share = node.size_bytes / total;
        const span = Math.max(2, Math.min(6, Math.round(share * 6)));
        const selected = selectedPaths.has(node.path);
        const selectedDescendants = flattenNodeChildren(node).filter((child) => selectedPaths.has(child.path));
        const hasSelectedDescendants = selectedDescendants.length > 0;
        const visuallySelected = selected || hasSelectedDescendants;
        const expanded = expandedPaths.has(node.path);
        const visibleChildren = node.children.filter((child) => child.size_bytes > 0);
        const hasChildren = visibleChildren.length > 0;
        const selectedDescendantBytes = selectedDescendants.reduce((sum, child) => sum + child.size_bytes, 0);
        return (
          <article
            key={node.path}
            className={`group min-w-0 p-4 text-left shadow-soft transition hover:-translate-y-0.5 ${safetyClass[node.safety]} ${
              visuallySelected ? "ring-2 ring-accent ring-offset-2 ring-offset-ink" : ""
            }`}
            style={{ minHeight: `${Math.max(132, 132 + share * 260)}px`, gridColumn: `span ${span} / span ${span}` }}
            title={`${node.path}\n${node.description}`}
          >
            <div className="flex h-full flex-col justify-between gap-4">
              <div>
                <div className="flex items-start justify-between gap-3">
                  <button
                    type="button"
                    className="flex min-w-0 flex-1 items-start gap-2 text-left"
                    onClick={() => {
                      if (hasChildren) toggleExpanded(node.path);
                      else onToggleNode(node);
                    }}
                    aria-expanded={hasChildren ? expanded : undefined}
                  >
                    {hasChildren && (
                      <ChevronDown className={`mt-0.5 shrink-0 transition ${expanded ? "" : "-rotate-90"}`} size={16} />
                    )}
                    <span className="break-words text-base font-black text-text">{localizeDynamicText(language, node.label)}</span>
                  </button>
                  <div className="flex shrink-0 items-center gap-2">
                    <span className="bg-panel/80 px-2 py-1 text-[11px] font-black uppercase tracking-wide">{copy.safety[node.safety]}</span>
                    {hasSelectedDescendants && (
                      <span className="bg-accent px-2 py-1 text-[11px] font-black uppercase tracking-wide text-panel">
                        {language === "vi" ? "Đã chọn cache con" : "Child cache selected"}
                      </span>
                    )}
                    <button
                      type="button"
                      className={`grid h-8 w-8 place-items-center rounded-full bg-panel/80 text-text transition hover:bg-panel ${
                        visuallySelected ? "text-accent" : node.safety === "Safe" ? "text-text" : "text-warn"
                      }`}
                      onClick={() => onToggleNode(node)}
                      aria-label={`${visuallySelected ? "Remove" : "Select"} ${localizeDynamicText(language, node.label)}`}
                    >
                      {visuallySelected ? <Check size={16} /> : <Plus size={16} />}
                    </button>
                  </div>
                </div>
                <p className="mt-2 line-clamp-3 text-sm text-muted">{localizeDynamicText(language, node.description)}</p>
                {hasSelectedDescendants && (
                  <p className="mt-2 text-sm font-black text-accent">
                    {language === "vi"
                      ? `Sẽ dọn ${formatBytes(selectedDescendantBytes)} cache an toàn bên trong. Folder cha được giữ nguyên.`
                      : `${formatBytes(selectedDescendantBytes)} of safe child cache selected. Parent folder stays untouched.`}
                  </p>
                )}
              </div>
              <div>
                <p className="text-2xl font-black text-text">{formatBytes(node.size_bytes)}</p>
                <p className="mt-1 text-xs text-muted">
                  {copy.filesFolders(node.file_count.toLocaleString(), node.dir_count.toLocaleString())}
                </p>
                {hasChildren && expanded && (
                  <div className="mt-4 grid gap-2 md:grid-cols-2">
                    {visibleChildren.map((child) => {
                        const childSelected = selectedPaths.has(child.path);
                        const childSelectedDescendants = flattenNodeChildren(child).filter((descendant) => selectedPaths.has(descendant.path));
                        const childVisuallySelected = childSelected || childSelectedDescendants.length > 0;
                        const childSelectedBytes = childSelectedDescendants.reduce((sum, descendant) => sum + descendant.size_bytes, 0);
                        return (
                          <button
                            key={child.path}
                            type="button"
                            className={`min-w-0 p-3 text-left transition hover:bg-panel/70 ${safetyClass[child.safety]} ${
                              childVisuallySelected ? "ring-2 ring-accent ring-offset-1 ring-offset-ink" : ""
                            }`}
                            onClick={() => onToggleNode(child)}
                            title={`${child.path}\n${child.description}`}
                          >
                            <div className="flex items-start justify-between gap-2">
                              <span className="min-w-0 break-words text-sm font-black text-text">{localizeDynamicText(language, child.label)}</span>
                              <span className="shrink-0 bg-panel/80 px-2 py-1 text-[10px] font-black uppercase tracking-wide">
                                {childVisuallySelected && child.safety !== "Safe"
                                  ? language === "vi"
                                    ? "ĐÃ CHỌN CACHE"
                                    : "CACHE SELECTED"
                                  : copy.safety[child.safety]}
                              </span>
                            </div>
                            <p className="mt-2 text-lg font-black text-text">{formatBytes(child.size_bytes)}</p>
                            <p className="mt-1 text-xs text-muted">
                              {copy.filesFolders(child.file_count.toLocaleString(), child.dir_count.toLocaleString())}
                            </p>
                            {childSelectedDescendants.length > 0 && (
                              <p className="mt-2 text-xs font-black text-accent">
                                {language === "vi"
                                  ? `Dọn ${formatBytes(childSelectedBytes)} cache con.`
                                  : `${formatBytes(childSelectedBytes)} child cache selected.`}
                              </p>
                            )}
                          </button>
                        );
                      })}
                  </div>
                )}
              </div>
            </div>
          </article>
        );
      })}
    </div>
  );
}
