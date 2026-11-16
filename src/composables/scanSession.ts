/** 磁盘分析扫描会话:模块级单例状态。
 *
 * 为什么放在模块级而不是 AnalyzerView 组件里:
 *  扫描(尤其 C 盘)是数分钟级的长任务,用户切到电脑清理页时希望它
 *  继续在后台跑、回来还能接着看。组件状态会随路由卸载销毁,只有
 *  模块级 ref 能在组件生命周期之外存活。
 *
 * 事件监听也在此模块绑定一次(App 生命周期内不卸载),后端事件
 * 始终推送进这组 ref;AnalyzerView 挂载/卸载只是"接上/断开显示"。 */
import { computed, ref, shallowRef } from "vue";

import { api, bindScanEvents } from "@/api";
import { t } from "@/i18n";
import { setMessage } from "@/composables/useToast";
import type {
  AppPhase,
  DeleteDone,
  DeleteItem,
  DeleteJobState,
  DeleteStarted,
  Progress as ProgressData,
  ScanResult,
  ScanTick,
} from "@/types";

// ---------- 扫描会话状态(App 生命周期单例) ----------
/** 当前阶段:idle 未扫描 / scanning 进行中 / done 完成 / failed 失败 */
export const phase = ref<AppPhase>("idle");
export const error = ref("");
/** 扫描结果整体替换,不做深度响应式:整棵树可能几十万节点,
 *  用 ref 会为每个节点建 Proxy,每秒快照刷新时是主要卡顿来源。 */
export const result = shallowRef<ScanResult | null>(null);
export const progress = ref<ProgressData>({
  dirs: 0,
  files: 0,
  bytes: 0,
  cur: "",
  startedAt: null,
  pausedSecs: 0,
  pausedAt: null,
});
export const paused = ref(false);
/** 扫描会话版本号:每次开始扫描自增,用于丢弃上一轮 Treemap 入场动画 */
export const epoch = ref(0);
/** 勾选的待删除路径(跨路由保留,事件处理需要访问它) */
export const selected = ref<string[]>([]);
/** 扫描期间已删除成功的路径:随后的快照仍会带回它们,先在前端过滤,
 *  等扫描结束(Done)时后端会把这些路径从终值里扣掉。 */
export const deletedPaths = ref<string[]>([]);
export const deletedSet = computed(() => new Set(deletedPaths.value));
/** 后台删除任务(跨路由保留:切走后删除继续跑,回来能看到进度) */
export const deleting = ref<DeleteJobState | null>(null);

// ---------- 操作反馈 ----------
// 直接复用全局 useToast(App.vue 壳统一渲染),不再自建气泡状态

// ---------- 后端事件 → 状态 ----------
let firstDeleteError = "";
let lastDeleteDone = 0;

/** 快照按帧合并:一帧内只应用最后一个结果,避免连续 JSON 刷新阻塞交互 */
let pendingSnapshot: ScanResult | null = null;
let snapshotScheduled = false;

function applySnapshot(p: ScanResult): void {
  pendingSnapshot = p;
  if (snapshotScheduled) return;
  snapshotScheduled = true;
  requestAnimationFrame(() => {
    snapshotScheduled = false;
    const next = pendingSnapshot;
    pendingSnapshot = null;
    // 不再判断组件是否存活:状态在模块级,组件卸载也照常更新,
    // 回到页面时直接呈现最新进度
    if (phase.value === "scanning" && next) result.value = next;
  });
}

const handlers = {
  onTick: (p: ScanTick): void => {
    progress.value.dirs = p.dirs;
    progress.value.files = p.files;
    progress.value.bytes = p.bytes;
    progress.value.cur = p.cur;
  },
  /** 扫描中每秒一次的部分快照:只替换展示数据,不改动下钻/勾选状态
   *  (父节点总是先于子节点入 Arena,节点索引在快照与终值间稳定) */
  onSnapshot: (p: ScanResult): void => applySnapshot(p),
  onDone: (p: ScanResult): void => {
    result.value = p;
    deletedPaths.value = [];
    phase.value = "done";
    selected.value = [];
    progress.value.startedAt = null;
    progress.value.pausedAt = null;
    paused.value = false;
  },
  /** 后台删除结束后 Rust 已把 apply_deleted 同步进模型,这里整体替换 */
  onUpdated: (p: ScanResult): void => {
    result.value = p;
  },
  onCancelled: (): void => {
    phase.value = "idle";
    result.value = null;
    paused.value = false;
    progress.value.startedAt = null;
    selected.value = [];
  },
  onFailed: (message: string): void => {
    phase.value = "failed";
    error.value = message;
    result.value = null;
    setMessage(t("toast.scanFailed", message));
  },
  onDeleteStarted: (p: DeleteStarted): void => {
    deleting.value = {
      done: p.done,
      total: p.total,
      ok: deleting.value?.ok ?? 0,
      current: p.path,
      cancelled: false,
    };
  },
  onDeleteItem: (p: DeleteItem): void => {
    lastDeleteDone = p.done;
    // 成功项从勾选集剔除;失败项放回勾选集便于直接重试
    const set = new Set(selected.value);
    if (p.error === null) {
      set.delete(p.path);
      // 扫描中删除:快照会把该文件再带回来,记下来在前端过滤掉
      if (phase.value === "scanning" && !deletedSet.value.has(p.path)) {
        deletedPaths.value = [...deletedPaths.value, p.path];
      }
    } else {
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
  onDeleteDone: (p: DeleteDone): void => {
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

// ---------- 事件绑定(App 生命周期内只绑一次,不随路由卸载) ----------
let binding: Promise<void> | null = null;

/** 确保后端事件监听已绑定。重复调用安全,只在首次真正执行。 */
export function ensureScanEvents(): Promise<void> {
  binding ??= bindScanEvents(handlers).then(() => undefined);
  return binding;
}

// ---------- 行为 ----------
/** 开始扫描指定根目录。注意:只更新会话状态,页面本地 UI(下钻栈、
 *  勾选、确认框)由调用方(AnalyzerView)自行重置。 */
export async function startScan(root: string): Promise<void> {
  if (phase.value === "scanning") return;

  epoch.value += 1;
  result.value = null;
  deletedPaths.value = [];
  selected.value = [];
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

/** 请求取消当前扫描(协作式:后端线程感知标志后自行退出) */
export function cancelScan(): void {
  paused.value = false;
  progress.value.pausedAt = null;
  void api.cancelScan();
}

/** 暂停/恢复扫描;记录暂停区间让状态栏耗时与速度冻结 */
export async function togglePause(): Promise<void> {
  if (phase.value !== "scanning") return;
  const next = !paused.value;
  paused.value = next;
  const now = performance.now();
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

/** 批量删除勾选路径(扫描进行中也允许,文件已列出即可清理) */
export async function performDelete(paths: string[]): Promise<void> {
  if (deleting.value) return;
  const list = [...paths];
  if (!list.length) return;
  firstDeleteError = "";
  lastDeleteDone = 0;
  deleting.value = { done: 0, total: list.length, ok: 0, current: null, cancelled: false };
  try {
    await api.deletePaths(list);
  } catch (e) {
    deleting.value = null;
    setMessage(String(e));
  }
}

/** 取消尚未执行的删除项(已删除的保留在回收站) */
export function cancelDelete(): void {
  if (!deleting.value) return;
  deleting.value = { ...deleting.value, cancelled: true };
  void api.cancelDelete();
  setMessage(t("toast.cancelDelete"));
}
