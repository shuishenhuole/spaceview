import { ref } from "vue";

import type { Lang } from "./types";

const zh = {
  "toolbar.target": "扫描目标: {0}",
  "toolbar.browse": "浏览…",
  "toolbar.custom": "自定义文件夹",
  "toolbar.start": "开始扫描",
  "toolbar.cancel": "取消",
  "toolbar.pause": "暂停",
  "toolbar.resume": "继续",
  "toolbar.deleting": "后台删除中…",
  "toolbar.light": "浅色",
  "toolbar.dark": "深色",
  "toolbar.motionOn": "动效:开",
  "toolbar.motionOff": "动效:关",
  "toolbar.langZh": "EN",
  "toolbar.langEn": "中文",

  "filter.label": "过滤",
  "filter.placeholder": "搜索目录 / 文件名",
  "filter.any": "不限大小",
  "filter.m10": "> 10MB",
  "filter.m100": "> 100MB",
  "filter.g1": "> 1GB",

  "tabs.files": "大文件 TopN",
  "tabs.ext": "按类型统计",
  "tabs.treemap": "Treemap",
  "scanning.live": "正在扫描 — 以下为已扫描部分的实时结果,完成后自动刷新",
  "scanning.wait": "正在扫描,请稍候…",
  "idle.hint": "选择扫描目标后点击「开始扫描」。",

  "tree.title": "目录树",
  "tree.search": "目录树 — 搜索「{0}」",
  "tree.empty": "无匹配目录",
  "tree.more": "共命中 {0} 个目录,仅显示前 {1} 个",
  "tree.smaller": "… 还有 {0} 个较小目录未列出",
  "tree.restricted": "受限",
  "tree.deleted": "已删除",
  "tree.locate": "单击在 Treemap 中定位",

  "files.selected": "已选 {0} 项",
  "files.matched": "匹配 {0} 条,渲染前 {1} 条",
  "files.hint": "↑↓ 移动光标 · 空格勾选 · Ctrl/Cmd+A 全选 · Delete 删除所选",
  "files.all": "全选",
  "files.size": "大小",
  "files.name": "文件(按路径)",
  "files.nameShort": "文件",
  "files.empty": "无匹配项,请调整搜索词或大小阈值",
  "files.rowHint": "双击打开所在目录,右键更多操作",
  "files.delete": "删除所选",

  "ext.title": "按扩展名占用排序(相对扫描到的文件总大小)",
  "ext.more": "… 其余 {0} 种扩展名合计占比不足尾部",

  "tm.empty": "空目录",
  "tm.current": "当前视图: {0} — {1}",
  "tm.own": "本目录文件",
  "tm.other": "其他 {0} 项",
  "tm.deleted": "已删除",
  "tm.pct": "占当前视图 {0}%",
  "tm.files": "{0} 个文件",
  "tm.back": "返回上一级",

  "status.ready": "就绪:选择存储位置或文件夹后点击「开始扫描」。",
  "status.paused": "已暂停",
  "status.scanning": "已扫描 {0} 个目录 / {1} 个文件,共 {2}",
  "status.current": "当前: {0}",
  "status.done": "扫描完成:{0} 个文件,合计 {1}。受限目录 {2} 项(已跳过)。",
  "status.failed": "扫描失败:{0}",
  "status.speed": "{0}s · {1} 文件/秒 · {2} MB/秒",
  "status.deleting": "正在后台删除到回收站…",
  "status.cancelling": "正在取消剩余删除项…",
  "status.cancelRemaining": "取消剩余",

  "menu.copy": "复制路径",
  "menu.reveal": "在文件管理器中打开",
  "menu.delete": "删除到回收站",

  "confirm.title": "确认删除到回收站",
  "confirm.description": "以下 {0} 项将被移入回收站,可随时恢复。",
  "confirm.more": "… 共 {0} 项",
  "confirm.ok": "删除到回收站",
  "confirm.cancel": "取消",

  "toast.copied": "已复制路径:{0}",
  "toast.revealFailed": "打开文件管理器失败:{0}",
  "toast.paused": "已暂停扫描,可随时继续",
  "toast.resumed": "已继续扫描",
  "toast.motionOn": "已恢复完整动画",
  "toast.motionOff": "已减少动画:关闭布局位移,仅保留状态反馈",
  "toast.deleted": "已删除 {0} 项到回收站",
  "toast.deletedPartial": "已删除 {0} 项;{1} 项失败:{2}",
  "toast.deleteCancelled": "{0};已取消剩余 {1} 项",
  "toast.blockedByDelete": "删除正在后台进行,请等待完成或取消后再扫描",
  "toast.blockedByScan": "扫描进行中,请等待完成后再删除",
  "toast.cancelDelete": "已请求取消剩余删除项",
  "toast.scanFailed": "扫描失败:{0}",
} as const;

export type I18nKey = keyof typeof zh;

const en: Record<I18nKey, string> = {
  "toolbar.target": "Target: {0}",
  "toolbar.browse": "Browse…",
  "toolbar.custom": "Custom folder",
  "toolbar.start": "Start Scan",
  "toolbar.cancel": "Cancel",
  "toolbar.pause": "Pause",
  "toolbar.resume": "Resume",
  "toolbar.deleting": "Deleting in background…",
  "toolbar.light": "Light",
  "toolbar.dark": "Dark",
  "toolbar.motionOn": "Motion: on",
  "toolbar.motionOff": "Motion: off",
  "toolbar.langZh": "EN",
  "toolbar.langEn": "中文",

  "filter.label": "Filter",
  "filter.placeholder": "Search folders / files",
  "filter.any": "Any size",
  "filter.m10": "> 10MB",
  "filter.m100": "> 100MB",
  "filter.g1": "> 1GB",

  "tabs.files": "Top Files",
  "tabs.ext": "By Type",
  "tabs.treemap": "Treemap",
  "scanning.live": "Scanning — live partial results, refreshed on completion",
  "scanning.wait": "Scanning, please wait…",
  "idle.hint": "Pick a scan target, then click “Start Scan”.",

  "tree.title": "Directory tree",
  "tree.search": "Directory tree — search “{0}”",
  "tree.empty": "No matching directories",
  "tree.more": "{0} dirs matched, showing the first {1}",
  "tree.smaller": "… {0} smaller dirs not listed",
  "tree.restricted": "restricted",
  "tree.deleted": "deleted",
  "tree.locate": "Click to locate in Treemap",

  "files.selected": "{0} selected",
  "files.matched": "matched {0}, rendering first {1}",
  "files.hint": "↑↓ move · Space toggle · Ctrl/Cmd+A select all · Delete removes selected",
  "files.all": "All",
  "files.size": "Size",
  "files.name": "File (path)",
  "files.nameShort": "File",
  "files.empty": "No matches — adjust the search term or size threshold",
  "files.rowHint": "Double-click opens folder, right-click for menu",
  "files.delete": "Delete selected",

  "ext.title": "Sorted by extension usage (vs total scanned bytes)",
  "ext.more": "… {0} more extensions share the remaining tail",

  "tm.empty": "Empty folder",
  "tm.current": "Current view: {0} — {1}",
  "tm.own": "Files in this folder",
  "tm.other": "{0} others",
  "tm.deleted": "deleted",
  "tm.pct": "{0}% of current view",
  "tm.files": "{0} files",
  "tm.back": "Go up",

  "status.ready": "Ready: pick a location or folder, then click “Start Scan”.",
  "status.paused": "Paused",
  "status.scanning": "Scanned {0} dirs / {1} files, {2} total",
  "status.current": "Current: {0}",
  "status.done": "Scan finished: {0} files, {1} total. {2} restricted directories skipped.",
  "status.failed": "Scan failed: {0}",
  "status.speed": "{0}s · {1} files/s · {2} MB/s",
  "status.deleting": "Deleting to Recycle Bin in background…",
  "status.cancelling": "Cancelling remaining deletes…",
  "status.cancelRemaining": "Cancel remaining",

  "menu.copy": "Copy path",
  "menu.reveal": "Open in file manager",
  "menu.delete": "Delete to Recycle Bin",

  "confirm.title": "Confirm delete to Recycle Bin",
  "confirm.description": "The following {0} item(s) will be moved to the Recycle Bin and can be restored.",
  "confirm.more": "… {0} items in total",
  "confirm.ok": "Delete to Recycle Bin",
  "confirm.cancel": "Cancel",

  "toast.copied": "Path copied: {0}",
  "toast.revealFailed": "Open file manager failed: {0}",
  "toast.paused": "Scan paused, resume anytime",
  "toast.resumed": "Scan resumed",
  "toast.motionOn": "Full motion restored",
  "toast.motionOff": "Reduced motion: layout movement off",
  "toast.deleted": "Deleted {0} item(s) to Recycle Bin",
  "toast.deletedPartial": "Deleted {0}; {1} failed: {2}",
  "toast.deleteCancelled": "{0}; cancelled {1} remaining",
  "toast.blockedByDelete": "Wait for the background delete to finish or cancel it before scanning",
  "toast.blockedByScan": "Scan in progress, delete after it finishes",
  "toast.cancelDelete": "Cancelling remaining delete items",
  "toast.scanFailed": "Scan failed: {0}",
};

const STORAGE_KEY = "spaceview.lang";
const stored = localStorage.getItem(STORAGE_KEY);
const lang = ref<Lang>(stored === "en" ? "en" : "zh");

export const currentLang = lang;

export function setLang(next: Lang): void {
  lang.value = next;
  localStorage.setItem(STORAGE_KEY, next);
}

export function toggleLang(): void {
  setLang(lang.value === "zh" ? "en" : "zh");
}

export function t(key: I18nKey, ...args: Array<string | number>): string {
  const template = (lang.value === "en" ? en[key] : zh[key]) ?? key;
  if (!args.length) return template;
  return template.replace(/\{(\d+)\}/g, (_m, i: string) => {
    const v = args[Number(i)];
    return v === undefined ? "" : String(v);
  });
}
