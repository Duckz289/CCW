import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { translations } from "../i18n";
import type { CleanupPreview } from "../types";
import { CleanupPreviewModal, ExportSuccessModal } from "./CleanupModals";

function preview(overrides: Partial<CleanupPreview> = {}): CleanupPreview {
  return {
    requested_paths: ["C:\\Claude\\Cache"],
    approved_paths: [
      {
        path: "C:\\Claude\\Cache",
        display_path: "%USERPROFILE%\\AppData\\Local\\Claude\\Cache",
        safety: "Safe",
        reason: "Rebuildable cache",
        estimated_bytes: 100,
        estimated_file_count: 2,
        estimated_directory_count: 1,
        requires_quarantine: false,
      },
    ],
    rejected_paths: [],
    estimated_bytes: 100,
    estimated_file_count: 2,
    estimated_directory_count: 1,
    protected_items_detected: false,
    claude_activity: "not_detected",
    cleanup_blocked: false,
    warnings: [],
    generated_at: new Date(0).toISOString(),
    ...overrides,
  };
}

describe("cleanup preview confirmation", () => {
  it("disables confirmation when backend approves no paths", () => {
    render(<CleanupPreviewModal preview={preview({ approved_paths: [] })} copy={translations.en} language="en" busy={false} onClose={vi.fn()} onConfirm={vi.fn()} />);
    expect(screen.getByRole("button", { name: translations.en.actions.confirm })).toBeDisabled();
  });

  it("requires explicit quarantine confirmation for Caution paths", () => {
    const onConfirm = vi.fn();
    const caution = preview({
      approved_paths: [{ ...preview().approved_paths[0], safety: "Caution", requires_quarantine: true }],
    });
    render(<CleanupPreviewModal preview={caution} copy={translations.en} language="en" busy={false} onClose={vi.fn()} onConfirm={onConfirm} />);
    const confirm = screen.getByRole("button", { name: translations.en.actions.confirm });
    expect(confirm).toBeDisabled();
    fireEvent.click(screen.getByRole("checkbox"));
    expect(confirm).toBeEnabled();
    fireEvent.click(confirm);
    expect(onConfirm).toHaveBeenCalledWith(true);
  });

  it("keeps confirmation blocked while Claude activity blocks cleanup", () => {
    render(<CleanupPreviewModal preview={preview({ cleanup_blocked: true, claude_activity: "background" })} copy={translations.en} language="en" busy={false} onClose={vi.fn()} onConfirm={vi.fn()} />);
    expect(screen.getByRole("button", { name: translations.en.actions.confirm })).toBeDisabled();
  });
});

describe("export success dialog", () => {
  it("shows the exported file and opens its containing folder on demand", () => {
    const onOpenFolder = vi.fn();
    render(
      <ExportSuccessModal
        path="C:\\Users\\Admin\\Documents\\claude-cache-report-20260715-120000.json"
        diagnostic={false}
        copy={translations.en}
        opening={false}
        onClose={vi.fn()}
        onOpenFolder={onOpenFolder}
      />,
    );
    expect(screen.getByText(/claude-cache-report-20260715-120000\.json/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: translations.en.exportModal.openFolder }));
    expect(onOpenFolder).toHaveBeenCalledOnce();
  });
});
