/** 与 Rust 侧 human_size 口径一致的展示工具 */

const UNITS = ["B", "KB", "MB", "GB", "TB", "PB"];

export function humanSize(bytes: number): string {
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${bytes} ${UNITS[0]}` : `${value.toFixed(2)} ${UNITS[unit]}`;
}

export function formatNumber(n: number): string {
  return n.toLocaleString("en-US");
}

/** Windows 与 POSIX 路径统一按 / 与 \ 切分 */
export function pathParts(p: string): string[] {
  return p.split(/[\\/]+/).filter(Boolean);
}

export function fileName(p: string): string {
  const parts = pathParts(p);
  return parts.length ? parts[parts.length - 1] : p;
}

export function parentDir(p: string): string {
  const idx = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/"));
  return idx <= 0 ? p : p.slice(0, idx);
}

/** 长路径只保留尾部,避免撑破布局 */
export function shortPath(p: string, max = 56): string {
  if (p.length <= max) return p;
  return `…${p.slice(p.length - (max - 1))}`;
}

export function extLabel(ext: string): string {
  if (ext === "(无扩展名)") return "(no ext)";
  return `.${ext.replace(/^\./, "")}`;
}

export function percent(part: number, total: number): number {
  return total > 0 ? (part / total) * 100 : 0;
}

/** 拼接 root + 一串目录名,得到节点的完整路径 */
export function dirPath(tree: { rootPath: string; nodes: { name: string; parent: number | null }[] }, idx: number): string {
  const names: string[] = [];
  let cur = idx;
  let guard = 0;
  while (cur >= 0 && guard++ < tree.nodes.length) {
    const node = tree.nodes[cur];
    if (node.parent === null) break;
    names.push(node.name);
    cur = node.parent;
  }
  names.reverse();
  let root = tree.rootPath.replace(/[\\/]+$/, "");
  for (const n of names) {
    root = root.includes("\\") ? `${root}\\${n}` : `${root}/${n}`;
  }
  return root;
}

/** 按像素宽度粗略截断文本(中文≈1em、ASCII≈0.55em) */
export function fitWidth(s: string, maxPx: number, fontPx = 12): string {
  let w = 0;
  let out = "";
  for (const ch of s) {
    const cw = ch.charCodeAt(0) < 128 ? fontPx * 0.55 : fontPx * 1.05;
    if (w + cw > maxPx) {
      out += "…";
      break;
    }
    w += cw;
    out += ch;
  }
  return out;
}
