import { ExternalLink, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import type { GithubIssue } from "../types";
import { formatDate } from "../lib/format";
import { localizeDynamicText, type Language, type translations } from "../i18n";

const ISSUE_NUMBERS = [43390, 37617, 34602];

interface IssueHubProps {
  copy: (typeof translations)[Language]["issues"];
  language: Language;
}

export function IssueHub({ copy, language }: IssueHubProps) {
  const [issues, setIssues] = useState<GithubIssue[]>([]);
  const [status, setStatus] = useState<"idle" | "loading" | "error">("idle");

  async function loadIssues() {
    setStatus("loading");
    try {
      const loaded = await Promise.all(
        ISSUE_NUMBERS.map(async (number) => {
          const response = await fetch(`https://api.github.com/repos/anthropics/claude-code/issues/${number}`);
          if (!response.ok) throw new Error(`GitHub issue ${number} returned ${response.status}`);
          return (await response.json()) as GithubIssue;
        }),
      );
      setIssues(loaded);
      setStatus("idle");
    } catch {
      setStatus("error");
    }
  }

  useEffect(() => {
    void loadIssues();
  }, []);

  return (
    <section className="space-y-4">
      <div className="flex items-center justify-between gap-3">
        <div>
          <h2 className="text-xl font-black text-text">{copy.title}</h2>
          <p className="mt-1 text-sm text-muted">{copy.subtitle}</p>
        </div>
        <button className="icon-button" type="button" onClick={loadIssues} title={copy.refresh}>
          <RefreshCw size={18} />
        </button>
      </div>

      {status === "error" && (
        <div className="rounded-[18px] bg-warn/15 p-4 text-sm text-warn shadow-soft">
          {copy.error}
        </div>
      )}

      {status === "loading" && issues.length === 0 ? (
        <div className="grid gap-3">
          {[0, 1, 2].map((item) => (
            <div key={item} className="h-24 animate-pulse rounded-[20px] bg-panel2 shadow-soft" />
          ))}
        </div>
      ) : (
        <div className="grid gap-3">
          {issues.map((issue) => (
            <a
              key={issue.number}
              href={issue.html_url}
              target="_blank"
              rel="noreferrer"
              className="rounded-[20px] bg-panel p-4 shadow-soft transition hover:-translate-y-0.5 hover:bg-panel2/80"
            >
              <div className="flex items-start justify-between gap-4">
                <div>
                  <p className="text-sm text-muted">anthropics/claude-code #{issue.number}</p>
                  <h3 className="mt-1 font-black text-text">{issue.title}</h3>
                  <p className="mt-2 text-sm text-muted">{copy.updated(formatDate(issue.updated_at))}</p>
                </div>
                <div className="flex shrink-0 items-center gap-2">
                  <span className={`px-2 py-1 text-xs font-black uppercase tracking-wide ${issue.state === "open" ? "bg-accent/15 text-accent" : "bg-muted/15 text-muted"}`}>
                    {localizeDynamicText(language, issue.state)}
                  </span>
                  <ExternalLink size={16} className="text-muted" />
                </div>
              </div>
            </a>
          ))}
        </div>
      )}
    </section>
  );
}
