import type { CacheNode, ScanResult } from "../types";

export function flattenNodes(nodes: CacheNode[]): CacheNode[] {
  return nodes.flatMap((node) => [node, ...flattenNodes(node.children)]);
}

export function hasNonZeroSize(node: CacheNode): boolean {
  return node.exists && node.size_bytes > 0;
}

export function defaultSafeSelection(scan: ScanResult | null): Set<string> {
  if (!scan) return new Set();
  return new Set(
    flattenNodes(scan.roots)
      .filter((node) => node.safety === "Safe" && node.default_cleanup && hasNonZeroSize(node))
      .map((node) => node.path),
  );
}

export function containsNodePath(parent: CacheNode, path: string): boolean {
  return parent.children.some((child) => child.path === path || containsNodePath(child, path));
}

export function cleanableDescendants(node: CacheNode): CacheNode[] {
  return flattenNodes(node.children).filter(
    (child) => child.exists && child.size_bytes > 0 && child.safety === "Safe" && child.default_cleanup,
  );
}

export function toggleExactPath(
  current: Set<string>,
  node: CacheNode,
  nodeByPath: Map<string, CacheNode>,
): Set<string> {
  const next = new Set(current);
  if (next.has(node.path)) {
    next.delete(node.path);
    return next;
  }
  for (const path of current) {
    const selectedNode = nodeByPath.get(path);
    if (selectedNode && (containsNodePath(node, path) || containsNodePath(selectedNode, node.path))) {
      next.delete(path);
    }
  }
  next.add(node.path);
  return next;
}
