<script setup lang="ts">
/** SpaceView 主界面编排层。
 *
 * 与旧版 egui 的 app.rs 保持同一分层原则:
 *  - 这里持有全部 UI 状态机(空闲/扫描中/完成/失败)、过滤/排序/下钻/勾选/偏好;
 *  - 子组件只负责渲染并上报动作,行为统一在此执行;
 *  - Rust 侧通过 scan://* / delete://* 事件推送进度与结果,这里只消费不阻塞;
 *  - 偏好(主题/语言/动效)用 localStorage 持久化,替代旧版 eframe Storage。 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { open as openFolderDialog } from "@tauri-apps/plugin-dialog";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import { gsap } from "gsap";
import { LoaderCircleIcon } from "@lucide/vue";

import { api, bindScanEvents } from "@/api";
import logoUrl from "@/assets/logo.png";
import { enterPanel, enterStagger, flashTheme } from "@/lib/motion";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { TooltipProvider } from "@/components/ui/tooltip";
import { currentLang, t, toggleLang } from "@/i18n";
import type {
  AppPhase,
  DeleteDone,
  DeleteItem,
  DeleteJobState,
  DeleteStarted,
  FileHit,
  Progress as ProgressData,
  RowAction,
  ScanResult,
  ScanTick,
  SortBy,
  TabKey,
} from "@/types";

import ConfirmDelete from "@/components/ConfirmDelete.vue";
import DirectoryTree from "@/components/DirectoryTree.vue";
import ExtensionStats from "@/components/ExtensionStats.vue";
import FilterBar from "@/components/FilterBar.vue";
import StatusBar from "@/components/StatusBar.vue";
import Toast from "@/components/Toast.vue";
import Toolbar from "@/components/Toolbar.vue";
import TopFiles from "@/components/TopFiles.vue";
import TreemapView from "@/components/TreemapView.vue";

const RENDER_LIMIT = 1000;
const ROOT = 0;

// ---------- 顶层状态 ----------
const drives = ref<string[]>([]);
const driveValue = ref("0");
const customPath = ref<string | null>(null);
const targetPath = computed(
  () => customPath.value ?? drives.value[Number(driveValue.value)] ?? drives.value[0] ?? "/",
);

const phase = ref<AppPhase>("idle");
const error = ref("");
const result = ref<ScanResult | null>(null);
const progress = ref<ProgressData>({
  dirs: 0,
  files: 0,
  bytes: 0,
  cur: "",
  startedAt: null,
  pausedSecs: 0,
  pausedAt: null,
});
const paused = ref(false);

/** 默认直接展示 Treemap(扫描中也能实时看到空间分布) */
const tab = ref<TabKey>("treemap");
const sortBy = ref<SortBy>("size");
const drill = ref<number[]>([ROOT]);
const selected = ref<string[]>([]);
const focusRow = ref<number | null>(null);
const filterText = ref("");
const minBytes = ref(0);

const deleting = ref<DeleteJobState | null>(null);
const confirmOpen = ref(false);
/** 扫描会话版本号:每次开始扫描自增,用于丢弃上一轮 Treemap 入场动画 */
const epoch = ref(0);

// ---------- 偏好(持久化) ----------
const dark = ref(localStorage.getItem("spaceview.theme") !== "light");
const reduceMotion = ref(localStorage.getItem("spaceview.motion") === "reduced");
/** 根容器引用:GSAP context 的作用域,卸载时统一 revert */
const shellRef = ref<HTMLElement | null>(null);

watch(
  dark,
  (v) => {
    document.documentElement.classList.toggle("dark", v);
    localStorage.setItem("spaceview.theme", v ? "dark" : "light");
    // 同步 Windows 原生标题栏(DWM 沉浸深色)与 WebView2 的 prefers-color-scheme
    void api.setTitlebarTheme(v).catch((e) =>
      console.warn("[SpaceView] set_titlebar_theme:", e),
    );
    // 切换瞬间给界面一个极轻的亮度反馈(减少动画时自动跳过)
    if (shellRef.value) flashTheme(shellRef.value);
  },
  { immediate: true },
);

watch(
  reduceMotion,
  (v) => {
    document.documentElement.classList.toggle("reduce-motion", v);
    localStorage.setItem("spaceview.motion", v ? "reduced" : "full");
  },
  { immediate: true },
);

watch(currentLang, (v) => {
  document.documentElement.lang = v === "zh" ? "zh-CN" : "en";
});

// ---------- 反馈气泡 ----------
const toast = ref<{ id: number; text: string } | null>(null);
let toastTimer: number | undefined;
let toastSeq = 0;

function setMessage(text: string): void {
  const id = ++toastSeq;
  toast.value = { id, text };
  if (toastTimer !== undefined) window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => {
    if (toast.value?.id === id) toast.value = null;
  }, 5000);
}

// ---------- 派生数据 ----------
/** 大文件列表:过滤 + 排序 + 渲染上限(与后端 TopN 降序口径一致) */
function filterFiles(res: ScanResult, limit: number): FileHit[] {
  const needle = filterText.value.trim().toLowerCase();
  const rows = res.topFiles.filter(
    (h) =>
      h.size >= minBytes.value &&
      (!needle || h.path.toLowerCase().includes(needle)),
  );
  if (sortBy.value === "name") rows.sort((a, b) => a.path.localeCompare(b.path));
  return rows.slice(0, limit);
}

const visibleFiles = computed<FileHit[]>(() =>
  result.value ? filterFiles(result.value, RENDER_LIMIT) : [],
);
const matchedCount = computed(() =>
  result.value ? filterFiles(result.value, Number.MAX_SAFE_INTEGER).length : 0,
);

// ---------- 后端事件 ----------
let firstDeleteError = "";
let lastDeleteDone = 0;

const handlers = {
  onTick: (p: ScanTick) => {
    progress.value.dirs = p.dirs;
    progress.value.files = p.files;
    progress.value.bytes = p.bytes;
    progress.value.cur = p.cur;
  },
  /** 扫描中每秒一次的部分快照:只替换展示数据,不改动下钻/勾选状态
   *  (父节点总是先于子节点入 Arena,节点索引在快照与终值间稳定) */
  onSnapshot: (p: ScanResult) => {
    if (phase.value === "scanning") result.value = p;
  },
  onDone: (p: ScanResult) => {
    result.value = p;
    phase.value = "done";
    selected.value = [];
    focusRow.value = null;
    progress.value.startedAt = null;
    progress.value.pausedAt = null;
    paused.value = false;
  },
  /** 后台删除结束后 Rust 已把 apply_deleted 同步进模型,这里整体替换 */
  onUpdated: (p: ScanResult) => {
    result.value = p;
  },
  onCancelled: () => {
    phase.value = "idle";
    result.value = null;
    paused.value = false;
    progress.value.startedAt = null;
  },
  onFailed: (message: string) => {
    phase.value = "failed";
    error.value = message;
    result.value = null;
    setMessage(t("toast.scanFailed", message));
  },
  onDeleteStarted: (p: DeleteStarted) => {
    deleting.value = {
      done: p.done,
      total: p.total,
      ok: deleting.value?.ok ?? 0,
      current: p.path,
      cancelled: false,
    };
  },
  onDeleteItem: (p: DeleteItem) => {
    lastDeleteDone = p.done;
    // 成功项从勾选集剔除;失败项放回勾选集便于直接重试
    const set = new Set(selected.value);
    if (p.error === null) set.delete(p.path);
    else {
      set.add(p.path);
      if (!firstDeleteError) firstDeleteError = p.error;
    }
    selected.value = [...set];
    if (deleting.value) {
      deleting.value = {
        ...deleting.value,
        done: p.done,
        total: p.total,
        ok: p.ok,
        current: null,
      };
    }
  },
  onDeleteDone: (p: DeleteDone) => {
    deleting.value = null;
    let msg =
      p.failed === 0
        ? t("toast.deleted", p.ok)
        : t("toast.deletedPartial", p.ok, p.failed, firstDeleteError);
    if (p.cancelled) {
      msg = t("toast.deleteCancelled", msg, Math.max(0, p.total - lastDeleteDone));
    }
    setMessage(msg);
    firstDeleteError = "";
  },
};

// ---------- 行为 ----------
async function startScan(): Promise<void> {
  if (deleting.value) {
    setMessage(t("toast.blockedByDelete"));
    return;
  }
  if (phase.value === "scanning") return;

  epoch.value += 1;
  const root = targetPath.value;
  result.value = null;
  drill.value = [ROOT];
  selected.value = [];
  focusRow.value = null;
  confirmOpen.value = false;
  error.value = "";
  paused.value = false;
  progress.value = {
    dirs: 0,
    files: 0,
    bytes: 0,
    cur: "",
    startedAt: performance.now(),
    pausedSecs: 0,
    pausedAt: null,
  };
  phase.value = "scanning";
  try {
    await api.startScan(root);
  } catch (e) {
    phase.value = "failed";
    error.value = String(e);
    setMessage(t("toast.scanFailed", String(e)));
  }
}

function cancelScan(): void {
  paused.value = false;
  progress.value.pausedAt = null;
  void api.cancelScan();
}

async function togglePause(): Promise<void> {
  if (phase.value !== "scanning") return;
  const next = !paused.value;
  paused.value = next;
  const now = performance.now();
  // 记录/结算暂停区间,让状态栏耗时与速度在暂停时冻结
  if (next) {
    progress.value.pausedAt = now;
  } else if (progress.value.pausedAt !== null) {
    progress.value.pausedSecs += (now - progress.value.pausedAt) / 1000;
    progress.value.pausedAt = null;
  }
  try {
    await api.setPaused(next);
  } catch {
    /* 后端不在线时仅保留本地暂停显示 */
  }
  setMessage(next ? t("toast.paused") : t("toast.resumed"));
}

async function browse(): Promise<void> {
  if (phase.value === "scanning" || deleting.value) return;
  try {
    const picked = await openFolderDialog({ directory: true, multiple: false });
    if (typeof picked === "string") customPath.value = picked;
  } catch (e) {
    setMessage(String(e));
  }
}

function toggleTheme(): void {
  dark.value = !dark.value;
}

function toggleMotion(): void {
  reduceMotion.value = !reduceMotion.value;
  setMessage(reduceMotion.value ? t("toast.motionOff") : t("toast.motionOn"));
}

async function performDelete(): Promise<void> {
  confirmOpen.value = false;
  if (phase.value !== "done" || deleting.value) return;
  const paths = [...selected.value];
  if (!paths.length) return;
  firstDeleteError = "";
  lastDeleteDone = 0;
  deleting.value = { done: 0, total: paths.length, ok: 0, current: null, cancelled: false };
  try {
    await api.deletePaths(paths);
  } catch (e) {
    deleting.value = null;
    setMessage(String(e));
  }
}

function cancelDelete(): void {
  if (!deleting.value) return;
  deleting.value = { ...deleting.value, cancelled: true };
  void api.cancelDelete();
  setMessage(t("toast.cancelDelete"));
}

async function handleRowAction(action: RowAction): Promise<void> {
  if (action.type === "copy") {
    try {
      // 优先走 Tauri 剪贴板插件;权限异常时回退到 WebView 的 Clipboard API
      try {
        await writeText(action.path);
      } catch {
        await navigator.clipboard.writeText(action.path);
      }
      setMessage(t("toast.copied", action.path));
    } catch (e) {
      setMessage(String(e));
    }
    return;
  }
  if (action.type === "reveal") {
    try {
      // 目录打开自身,文件在所在目录中选中
      if (action.isDir) await openPath(action.path);
      else await revealItemInDir(action.path);
    } catch (e) {
      setMessage(t("toast.revealFailed", String(e)));
    }
    return;
  }
  // 删除:复用"勾选 → 确认对话框 → performDelete"流程
  if (deleting.value || phase.value === "scanning") {
    setMessage(deleting.value ? t("toast.blockedByDelete") : t("toast.blockedByScan"));
    return;
  }
  if (phase.value !== "done") return;
  if (!(selected.value.length > 1 && selected.value.includes(action.path))) {
    selected.value = [action.path];
  }
  confirmOpen.value = true;
}

/** 沿 parent 链重建下钻栈,把 Treemap 定位到指定目录并切换 Tab */
function navigateTreemap(idx: number): void {
  const r = result.value;
  if (!r) return;
  const stack: number[] = [];
  let cur = idx;
  let guard = 0;
  while (cur >= 0 && guard++ < r.tree.nodes.length) {
    stack.push(cur);
    const p = r.tree.nodes[cur].parent;
    if (p === null) break;
    cur = p;
  }
  stack.reverse();
  drill.value = stack.length ? stack : [ROOT];
  tab.value = "treemap";
}

function drillInto(idx: number): void {
  drill.value = [...drill.value, idx];
}

function backTo(level: number): void {
  drill.value = drill.value.slice(0, Math.max(1, level + 1));
}

function toggleOne(path: string): void {
  const set = new Set(selected.value);
  if (set.has(path)) set.delete(path);
  else set.add(path);
  selected.value = [...set];
}

// ---------- 键盘快捷键 ----------
function onKeydown(e: KeyboardEvent): void {
  const el = e.target as HTMLElement | null;
  // 输入框获得焦点时不拦截导航键
  if (
    el &&
    (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable)
  ) {
    return;
  }
  if (e.key === "Delete") {
    if (selected.value.length && phase.value === "done" && !deleting.value) {
      e.preventDefault();
      confirmOpen.value = true;
    }
    return;
  }
  if (tab.value === "treemap" && e.key === "Backspace" && drill.value.length > 1) {
    e.preventDefault();
    drill.value = drill.value.slice(0, -1);
    return;
  }
  if (tab.value !== "files" || !visibleFiles.value.length) return;

  const rows = visibleFiles.value;
  if (e.key === "ArrowDown") {
    e.preventDefault();
    focusRow.value = focusRow.value === null ? 0 : Math.min(rows.length - 1, focusRow.value + 1);
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    focusRow.value = focusRow.value === null ? 0 : Math.max(0, focusRow.value - 1);
  } else if (e.key === " " || e.code === "Space") {
    e.preventDefault();
    if (focusRow.value !== null && rows[focusRow.value]) toggleOne(rows[focusRow.value].path);
  } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
    e.preventDefault();
    const allSelected = rows.every((h) => selected.value.includes(h.path));
    if (allSelected) {
      selected.value = selected.value.filter((p) => !rows.some((h) => h.path === p));
    } else {
      const set = new Set(selected.value);
      for (const h of rows) set.add(h.path);
      selected.value = [...set];
    }
  }
}

// ---------- 动效 ----------
let animContext: ReturnType<typeof gsap.context> | null = null;

/** 结果从"无"到"有"的瞬间:左右面板依次淡入上移 */
watch(result, async (next, prev) => {
  if (!next || prev) return;
  await nextTick();
  enterPanel(shellRef.value?.querySelectorAll("[data-result-panel]") ?? null, {
    stagger: 0.09,
    y: 16,
    duration: 0.45,
  });
});

/** Tab 切换:内容区淡入 */
watch(tab, async () => {
  await nextTick();
  enterPanel(shellRef.value?.querySelector("[data-tab-panel]") ?? null, {
    y: 8,
    duration: 0.3,
  });
});

// ---------- 生命周期 ----------
let unlisteners: UnlistenFn[] = [];

onMounted(async () => {
  try {
    drives.value = await api.listDrives();
  } catch {
    drives.value = [];
  }
  // 默认选中系统盘,找不到则取第一个
  const idx = drives.value.findIndex((d) => d.toUpperCase().startsWith("C:"));
  driveValue.value = String(idx >= 0 ? idx : 0);
  unlisteners = await bindScanEvents(handlers);
  window.addEventListener("keydown", onKeydown);
  // 窗口完全就绪后再下发一次,确保 DWM 属性在标题栏可见后生效
  void api.setTitlebarTheme(dark.value).catch(() => {});

  // 首屏:工具栏 / 主面板 / 状态栏依次入场
  await nextTick();
  animContext = gsap.context(() => {
    enterStagger(shellRef.value?.querySelectorAll("[data-enter]") ?? null, {
      stagger: 0.09,
      y: 16,
      duration: 0.55,
    });
  }, shellRef.value ?? undefined);
});

onBeforeUnmount(() => {
  for (const off of unlisteners) off();
  window.removeEventListener("keydown", onKeydown);
  if (toastTimer !== undefined) window.clearTimeout(toastTimer);
  animContext?.revert();
});
</script>

<template>
  <TooltipProvider :delay-duration="350">
    <div ref="shellRef" class="app-shell" :class="{ 'reduce-motion': reduceMotion }">
      <Toolbar
        data-enter
        :drives="drives"
        :drive-value="driveValue"
        :custom-path="customPath"
        :target-path="targetPath"
        :phase="phase"
        :paused="paused"
        :deleting="deleting !== null"
        :dark="dark"
        :reduce-motion="reduceMotion"
        @select-drive="driveValue = $event"
        @browse="browse"
        @start="startScan"
        @cancel="cancelScan"
        @toggle-pause="togglePause"
        @toggle-theme="toggleTheme"
        @toggle-motion="toggleMotion"
        @toggle-lang="toggleLang"
      />

      <FilterBar
        v-if="result"
        data-enter
        class="mx-3 mt-2"
        :filter-text="filterText"
        :min-bytes="minBytes"
        @update:filter-text="filterText = $event"
        @update:min-bytes="minBytes = $event"
      />

      <div class="mx-3 mt-2 mb-2 flex min-h-0 flex-1 gap-3">
        <!-- 左侧:目录树(有结果时显示,单击联动 Treemap) -->
        <aside
          v-if="result"
          data-result-panel
          class="panel flex w-[330px] shrink-0 flex-col overflow-hidden"
        >
          <DirectoryTree
            :tree="result.tree"
            :needle="filterText"
            :min-bytes="minBytes"
            @navigate="navigateTreemap"
            @action="handleRowAction"
          />
        </aside>

        <!-- 中央:三大视图 -->
        <main
          data-enter
          data-result-panel
          class="panel flex min-w-0 flex-1 flex-col overflow-hidden"
        >
          <div
            v-if="!result"
            class="flex flex-1 flex-col items-center justify-center gap-3 text-center"
          >
            <img :src="logoUrl" alt="SpaceView" class="size-16 rounded-2xl shadow-lg" />
            <h2 class="text-lg font-semibold tracking-tight">SpaceView</h2>
            <p class="max-w-sm text-sm text-muted-foreground">
              {{ phase === "scanning" ? t("scanning.wait") : t("idle.hint") }}
            </p>
            <LoaderCircleIcon
              v-if="phase === 'scanning'"
              class="size-5 animate-spin text-brand"
            />
          </div>

          <Tabs v-else v-model="tab" class="flex min-h-0 flex-1 flex-col p-3">
            <div class="flex shrink-0 items-center gap-3">
              <span
                v-if="phase === 'scanning'"
                class="hidden text-[11px] italic text-muted-foreground sm:inline"
              >
                {{ t("scanning.live") }}
              </span>
              <TabsList>
                <TabsTrigger value="files">{{ t("tabs.files") }}</TabsTrigger>
                <TabsTrigger value="extensions">{{ t("tabs.ext") }}</TabsTrigger>
                <TabsTrigger value="treemap">{{ t("tabs.treemap") }}</TabsTrigger>
              </TabsList>
            </div>

            <TabsContent value="files" data-tab-panel class="mt-2 min-h-0 flex-1">
              <TopFiles
                :rows="visibleFiles"
                :matched="matchedCount"
                :render-limit="RENDER_LIMIT"
                :sort-by="sortBy"
                :selected="selected"
                :focus-row="focusRow"
                @update:sort-by="sortBy = $event"
                @update:selected="selected = $event"
                @update:focus-row="focusRow = $event"
                @action="handleRowAction"
                @request-delete="confirmOpen = true"
              />
            </TabsContent>

            <TabsContent value="extensions" data-tab-panel class="mt-2 min-h-0 flex-1">
              <ExtensionStats :result="result" />
            </TabsContent>

            <TabsContent value="treemap" data-tab-panel class="mt-2 min-h-0 flex-1">
              <TreemapView
                :tree="result.tree"
                :drill="drill"
                :epoch="epoch"
                :dark="dark"
                @drill="drillInto"
                @back="backTo"
                @action="handleRowAction"
              />
            </TabsContent>
          </Tabs>
        </main>
      </div>

      <StatusBar
        data-enter
        class="mx-3 mb-3"
        :phase="phase"
        :paused="paused"
        :progress="progress"
        :result="result"
        :error="error"
        :deleting="deleting"
        @cancel-delete="cancelDelete"
      />

      <ConfirmDelete
        :open="confirmOpen"
        :paths="selected"
        @update:open="confirmOpen = $event"
        @confirm="performDelete"
      />
      <Toast :message="toast?.text ?? null" />
    </div>
  </TooltipProvider>
</template>
