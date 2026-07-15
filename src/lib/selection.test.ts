import { describe, expect, it } from "vitest";
import { cleanableDescendants, defaultSafeSelection, flattenNodes, toggleExactPath } from "./selection";
import type { CacheNode, ScanResult } from "../types";

function node(path: string, safety: CacheNode["safety"], defaults = false, children: CacheNode[] = []): CacheNode {
  return {
    label: path,
    path,
    size_bytes: 10,
    file_count: 1,
    dir_count: children.length,
    exists: true,
    safety,
    default_cleanup: defaults,
    description: path,
    children,
  };
}

describe("cleanup selection policy", () => {
  const safe = node("/Claude/Cache", "Safe", true);
  const caution = node("/Claude/unknown", "Caution");
  const protectedNode = node("/Claude", "NotRecommended", false, [safe, caution]);
  const scan: ScanResult = {
    platform: "test",
    scanned_at: new Date(0).toISOString(),
    total_bytes: 30,
    roots: [protectedNode],
    claude_running: false,
    claude_activity: "not_detected",
    warnings: [],
  };

  it("selects only Safe default paths", () => {
    expect([...defaultSafeSelection(scan)]).toEqual([safe.path]);
    expect(cleanableDescendants(protectedNode).map((item) => item.path)).toEqual([safe.path]);
  });

  it("keeps parent and child selections from overlapping", () => {
    const byPath = new Map(flattenNodes(scan.roots).map((item) => [item.path, item]));
    const selectedChild = toggleExactPath(new Set(), safe, byPath);
    const selectedParent = toggleExactPath(selectedChild, protectedNode, byPath);
    expect([...selectedParent]).toEqual([protectedNode.path]);
  });
});
