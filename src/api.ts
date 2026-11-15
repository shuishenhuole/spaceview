import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  DeleteDone,
  DeleteItem,
  DeleteStarted,
  ScanResult,
  ScanTick,
} from "./types";

/** Rust commands */
export const api = {
  listDrives: () => invoke<string[]>("list_drives"),
  startScan: (root: string) => invoke<void>("start_scan", { root }),
  cancelScan: () => invoke<void>("cancel_scan"),
  setPaused: (paused: boolean) => invoke<void>("set_paused", { paused }),
  deletePaths: (paths: string[]) => invoke<void>("delete_paths", { paths }),
  cancelDelete: () => invoke<void>("cancel_delete"),
  /** 同步 Windows 原生标题栏深/浅色(DWM 沉浸深色属性) */
  setTitlebarTheme: (dark: boolean) => invoke<void>("set_titlebar_theme", { dark }),
};

export interface ScanEventHandlers {
  onTick: (p: ScanTick) => void;
  onSnapshot: (p: ScanResult) => void;
  onDone: (p: ScanResult) => void;
  onUpdated: (p: ScanResult) => void;
  onCancelled: () => void;
  onFailed: (message: string) => void;
  onDeleteStarted: (p: DeleteStarted) => void;
  onDeleteItem: (p: DeleteItem) => void;
  onDeleteDone: (p: DeleteDone) => void;
}

/** 订阅全部后端事件,返回用于卸载的函数列表 */
export function bindScanEvents(h: ScanEventHandlers): Promise<UnlistenFn[]> {
  return Promise.all([
    listen<ScanTick>("scan://tick", (e) => h.onTick(e.payload)),
    listen<ScanResult>("scan://snapshot", (e) => h.onSnapshot(e.payload)),
    listen<ScanResult>("scan://done", (e) => h.onDone(e.payload)),
    listen<ScanResult>("scan://updated", (e) => h.onUpdated(e.payload)),
    listen<null>("scan://cancelled", () => h.onCancelled()),
    listen<string>("scan://failed", (e) => h.onFailed(e.payload)),
    listen<DeleteStarted>("delete://started", (e) => h.onDeleteStarted(e.payload)),
    listen<DeleteItem>("delete://item", (e) => h.onDeleteItem(e.payload)),
    listen<DeleteDone>("delete://done", (e) => h.onDeleteDone(e.payload)),
  ]);
}
