/** Squarified Treemap 布局算法(纯计算,从旧版 Rust engine/treemap.rs 移植)。
 *  参考 Bruls, Huizing, van Wijk "Squarified Treemaps"。 */

import type { Tree } from "../types";

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

type Weighted<T> = [T, number];
/** [标识, 权重, 矩形] */
export type LayoutResult<T> = [T, number, Rect];

/** 对 items(标识 + 权重)在 rect 内做 squarified 布局,权重 <= 0 的项被丢弃 */
export function squarify<T>(items: Array<Weighted<T>>, rect: Rect): Array<LayoutResult<T>> {
  const sorted = items
    .filter(([, s]) => s > 0)
    .slice()
    .sort((a, b) => b[1] - a[1]);
  if (!sorted.length) return [];
  const out: Array<LayoutResult<T>> = [];
  layout(sorted, rect, out);
  return out;
}

function layout<T>(items: Array<Weighted<T>>, rect: Rect, out: Array<LayoutResult<T>>): void {
  if (!items.length || rect.w <= 0.5 || rect.h <= 0.5) {
    if (items.length) out.push([items[0][0], items[0][1], rect]);
    return;
  }
  const total = items.reduce((s, [, v]) => s + v, 0);
  if (total === 0) return;

  const px2 = (rect.w * rect.h) / total;
  const horizontal = rect.h <= rect.w;
  const shorter = (horizontal ? rect.h : rect.w) as number;

  // 贪心成行:逐行加入项,直到最差纵横比变差
  let rowLen = 0;
  let rowSum = 0;
  let bestWorst = Number.POSITIVE_INFINITY;
  for (let i = 0; i < items.length; i++) {
    const trial = rowSum + items[i][1];
    if (i > 0) {
      const w = worstRatio(items.slice(0, i + 1), shorter * shorter, trial);
      if (w > bestWorst) break;
      bestWorst = w;
    }
    rowLen = i + 1;
    rowSum = trial;
  }
  if (rowLen === 0) {
    rowLen = 1;
    rowSum = items[0][1];
  }

  const thickness = Math.max(0.0001, (rowSum * px2) / shorter);
  let cursor = 0;
  for (let i = 0; i < rowLen; i++) {
    const [key, size] = items[i];
    const extent = (size * px2) / thickness;
    const r: Rect = horizontal
      ? { x: rect.x, y: rect.y + cursor, w: thickness, h: extent }
      : { x: rect.x + cursor, y: rect.y, w: extent, h: thickness };
    out.push([key, size, r]);
    cursor += extent;
  }

  const rest: Rect = horizontal
    ? { x: rect.x + thickness, y: rect.y, w: Math.max(0, rect.w - thickness), h: rect.h }
    : { x: rect.x, y: rect.y + thickness, w: rect.w, h: Math.max(0, rect.h - thickness) };
  layout(items.slice(rowLen), rest, out);
}

function worstRatio<T>(items: Array<Weighted<T>>, s2: number, rowSum: number): number {
  let mx = 1;
  let mn = Number.POSITIVE_INFINITY;
  for (const [, s] of items) {
    if (s > mx) mx = s;
    if (s < mn) mn = s;
  }
  const sum2 = rowSum * rowSum;
  if (sum2 <= 0 || s2 <= 0 || !Number.isFinite(mn) || mn <= 0) return Number.POSITIVE_INFINITY;
  return Math.max((s2 * mx) / sum2, sum2 / (s2 * mn));
}

export type BlockKind = "dir" | "files" | "other";

export interface TreemapBlock {
  kind: BlockKind;
  /** kind=dir 时为节点索引,其余为 -1 */
  idx: number;
  size: number;
  name: string;
  deleted: boolean;
  /** kind=other 时被合并的子目录数 */
  merged: number;
}

export interface BlockSet {
  blocks: TreemapBlock[];
  curSize: number;
  ownFiles: number;
}

/** 当前视图的块集合:子目录 + 本目录直属文件(灰色) + 超限合并块。
 *  对齐旧版逻辑:已删除目录以固定小权重占位显示。 */
export function collectBlocks(tree: Tree, current: number, limit = 160): BlockSet {
  const node = tree.nodes[current];
  const curSize = node.size;
  const tombWeight = Math.max(Math.floor(curSize / 40), 1);

  let childrenSum = 0;
  const dirs: TreemapBlock[] = node.children.map((c) => {
    const n = tree.nodes[c];
    childrenSum += n.size;
    return {
      kind: "dir" as const,
      idx: c,
      size: n.deleted ? tombWeight : n.size,
      name: n.name,
      deleted: n.deleted,
      merged: 0,
    };
  });
  dirs.sort((a, b) => b.size - a.size);

  const ownFiles = Math.max(0, curSize - childrenSum);
  const blocks: TreemapBlock[] = dirs.slice(0, limit);
  if (dirs.length > limit) {
    const rest = dirs.slice(limit);
    blocks.push({
      kind: "other",
      idx: -1,
      size: rest.reduce((s, b) => s + b.size, 0),
      name: "",
      deleted: false,
      merged: rest.length,
    });
  }
  if (ownFiles > 0) {
    blocks.push({ kind: "files", idx: -1, size: ownFiles, name: "", deleted: false, merged: 0 });
  }
  return { blocks, curSize, ownFiles };
}

/** 马卡龙调色板(与旧版一致),直属文件块用中性灰 */
export function blockColor(index: number, kind: BlockKind, dark: boolean): string {
  if (kind === "files") return dark ? "#3e4351" : "#d7dbe3";
  if (kind === "other") return dark ? "#4a4f60" : "#c9cfda";
  const palette = [
    [176, 200, 226],
    [182, 216, 182],
    [235, 197, 170],
    [213, 180, 213],
    [170, 214, 214],
    [226, 200, 160],
    [190, 175, 226],
    [180, 214, 180],
  ];
  const [r, g, b] = palette[index % palette.length];
  return `rgb(${r}, ${g}, ${b})`;
}

/** 依据底色亮度选择文字颜色 */
export function blockTextColor(rgb: string): string {
  const m = rgb.match(/(\d+),\s*(\d+),\s*(\d+)/);
  if (!m) return "#111827";
  const lum = 0.2126 * Number(m[1]) + 0.7152 * Number(m[2]) + 0.0722 * Number(m[3]);
  return lum > 140 ? "#1b2130" : "#f8fafc";
}
