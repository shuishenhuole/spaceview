/** 与 src-tauri/src/engine/dto.rs 一一对应的数据类型(camelCase) */

export interface DirNode {
  parent: number | null;
  name: string;
  size: number;
  fileCount: number;
  children: number[];
  denied: boolean;
  deleted: boolean;
}

export interface Tree {
  rootPath: string;
  nodes: DirNode[];
}

export interface FileHit {
  path: string;
  size: number;
  dirIdx: number;
}

export interface ExtStat {
  ext: string;
  count: number;
  bytes: number;
}

export interface ScanResult {
  tree: Tree;
  topFiles: FileHit[];
  extStats: ExtStat[];
  denied: string[];
  totalFiles: number;
  totalBytes: number;
}

export interface ScanTick {
  dirs: number;
  files: number;
  bytes: number;
  cur: string;
}

export interface DeleteStarted {
  path: string;
  done: number;
  total: number;
}

export interface DeleteItem {
  path: string;
  error: string | null;
  ok: number;
  done: number;
  total: number;
}

export interface DeleteDone {
  ok: number;
  failed: number;
  cancelled: boolean;
  total: number;
}

export type AppPhase = "idle" | "scanning" | "done" | "failed";
export type TabKey = "files" | "extensions" | "treemap";
export type SortBy = "size" | "name";
export type Lang = "zh" | "en";

export interface Filter {
  needle: string;
  minBytes: number;
}

export interface Progress {
  dirs: number;
  files: number;
  bytes: number;
  cur: string;
  /** performance.now() 起点;null 表示未开始 */
  startedAt: number | null;
  /** 累计暂停时长(秒) */
  pausedSecs: number;
  /** 本次暂停起点;null 表示未暂停 */
  pausedAt: number | null;
}

export interface DeleteJobState {
  done: number;
  total: number;
  ok: number;
  current: string | null;
  cancelled: boolean;
}

export type RowActionType = "copy" | "reveal" | "delete";

export interface RowAction {
  type: RowActionType;
  path: string;
  /** 目录用 openPath 打开自身,文件用 revealItemInDir 在所在目录选中 */
  isDir: boolean;
}
