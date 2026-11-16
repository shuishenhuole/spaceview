<script setup lang="ts">
/** 电脑清理视图:扫描常见位置的临时 / 缓存 / 残留 / 大文件,勾选后一键
 *  移入回收站。与主分析器的删除走同一套 Tauri commands。 */
import {
  computed,
  onBeforeUnmount,
  onMounted,
  ref,
  type Ref,
} from "vue";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { LoaderCircleIcon, PackageOpenIcon, SearchIcon, Trash2Icon } from "@lucide/vue";

import { api, junkApi, bindDeleteEvents, bindJunkEvents } from "@/api";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Badge } from "@/components/ui/badge";
import logoUrl from "@/assets/logo.png";
import AppNav from "@/components/AppNav.vue";
import ScanLoading from "@/components/ScanLoading.vue";
import { t } from "@/i18n";
import { pageEnter, pageLeave } from "@/lib/motion";
import { setMessage } from "@/composables/useToast";
import { fileName, humanSize, parentDir } from "@/lib/format";
import type { JunkItem, JunkRoot, RowAction, InstalledApp } from "@/types";

import ConfirmDelete from "@/components/ConfirmDelete.vue";
import RowContextMenu from "@/components/RowContextMenu.vue";

const RENDER_LIMIT = 500;

const roots: Ref<JunkRoot[]> = ref([]);
const selectedRoots: Ref<string[]> = ref([]); // all by default
const scanning = ref(false);
const scanned = ref(0);
const items: Ref<JunkItem[]> = ref([]);
const checked: Ref<string[]> = ref([]);
const filterKind = ref<"all" | "temp" | "cache" | "leftover" | "large" | "log">(
  "all",
);
const errorMsg = ref("");
const confirmOpen = ref(false);
const page = ref<"junk" | "software">("junk");
const deleting = ref<{ done: number; total: number; ok: number; current: string | null } | null>(null);
const deleteErrors = ref<string[]>([]);
const leftoverRoots = ref<string[]>([]);
const leftoverItems = ref<JunkItem[]>([]);
const leftoverScanning = ref(false);
const leftoverTick = ref(0);
const junkScanMode = ref<"junk" | "leftover">("junk");
const selectedLeftovers = ref<string[]>([]);
const software = ref<InstalledApp[]>([]);
const softwareSearch = ref("");
const softwareLoading = ref(false);
const uninstalling = ref<string | null>(null);

let unlisteners: UnlistenFn[] = [];

const KIND_LABEL: Record<string, string> = {
  all: t("cleaner.kind.all"),
  temp: t("cleaner.kind.temp"),
  cache: t("cleaner.kind.cache"),
  leftover: t("cleaner.kind.leftover"),
  large: t("cleaner.kind.large"),
  log: t("cleaner.kind.log"),
};

const kindVariant = (kind: string): "destructive" | "secondary" | "outline" => {
  switch (kind) {
    case "temp":
    case "large":
      return "destructive";
    case "cache":
    case "leftover":
    case "log":
      return "secondary";
    default:
      return "outline";
  }
};

onMounted(async () => {
  try {
    roots.value = await junkApi.junkRoots();
    selectedRoots.value = roots.value.map((r) => r.path);
  } catch (e) {
    errorMsg.value = String(e);
  }
  const junkListeners = await bindJunkEvents({
    onTick: (p: { scanned: number }) => {
      if (junkScanMode.value === "leftover") leftoverTick.value = p.scanned;
      else scanned.value = p.scanned;
    },
    onDone: (p: { items: JunkItem[] }) => {
      if (junkScanMode.value === "leftover") {
        leftoverItems.value = p.items;
        leftoverScanning.value = false;
      } else {
        items.value = p.items;
        scanning.value = false;
      }
    },
    onFailed: (m: string) => {
      scanning.value = false;
      errorMsg.value = m;
    },
  });
  const deleteListeners = await bindDeleteEvents({
    onStarted: (p) => {
      deleting.value = {
        done: p.done,
        total: p.total,
        ok: deleting.value?.ok ?? 0,
        current: p.path,
      };
    },
    onItem: (p) => {
      if (p.error) deleteErrors.value.push(`${p.path}: ${p.error}`);
      const set = new Set(checked.value);
      if (p.error === null) {
        set.delete(p.path);
        items.value = items.value.filter((item) => item.path !== p.path);
        leftoverItems.value = leftoverItems.value.filter((item) => item.path !== p.path);
      }
      checked.value = [...set];
      deleting.value = { done: p.done, total: p.total, ok: p.ok, current: null };
    },
    onDone: (p) => {
      deleting.value = null;
      if (p.failed) errorMsg.value = deleteErrors.value[0] ?? `${p.failed} delete(s) failed`;
      deleteErrors.value = [];
    },
  });
  unlisteners = [...junkListeners, ...deleteListeners];
});

onBeforeUnmount(() => {
  for (const off of unlisteners) off();
});

const allRootsSelected = computed(
  () => roots.value.length > 0 && roots.value.every((r) => selectedRoots.value.includes(r.path)),
);

const filteredItems = computed<JunkItem[]>(() => {
  const k = filterKind.value;
  if (k === "all") return items.value;
  return items.value.filter((i) => i.kind === k);
});

const visibleItems = computed(() => filteredItems.value.slice(0, RENDER_LIMIT));

const totalBytes = computed(() =>
  filteredItems.value.reduce((s, i) => s + i.size, 0),
);

function toggleRoot(path: string): void {
  const set = new Set(selectedRoots.value);
  if (set.has(path)) set.delete(path);
  else set.add(path);
  selectedRoots.value = [...set];
}

async function startScan(): Promise<void> {
  if (scanning.value) return;
  const rootsPaths = roots.value
    .filter((r: JunkRoot) => selectedRoots.value.includes(r.path))
    .map((r: JunkRoot) => r.path);
  if (!rootsPaths.length) {
    errorMsg.value = t("cleaner.noRoots");
    return;
  }
  items.value = [];
  checked.value = [];
  errorMsg.value = "";
  scanning.value = true;
  junkScanMode.value = "junk";
  scanned.value = 0;
  try {
    await junkApi.scanJunk(rootsPaths);
  } catch (e) {
    scanning.value = false;
    errorMsg.value = String(e);
  }
}

function cancelScan(): void {
  void junkApi.cancelJunk();
}

function toggleItem(path: string): void {
  const set = new Set(checked.value);
  if (set.has(path)) set.delete(path);
  else set.add(path);
  checked.value = [...set];
}

function selectAllItems(): void {
  const set = new Set(visibleItems.value.map((i) => i.path));
  checked.value = [...set];
}

function clearItems(): void {
  checked.value = [];
}

async function performDelete(): Promise<void> {
  confirmOpen.value = false;
  const paths = [...checked.value];
  if (!paths.length || deleting.value) return;
  deleteErrors.value = [];
  deleting.value = { done: 0, total: paths.length, ok: 0, current: null };
  try {
    await api.deletePaths(paths);
  } catch (e) {
    deleting.value = null;
    errorMsg.value = String(e);
  }
}

const filteredSoftware = computed(() => {
  const query = softwareSearch.value.trim().toLocaleLowerCase();
  if (!query) return software.value;
  return software.value.filter((app) =>
    `${app.name} ${app.publisher} ${app.version}`.toLocaleLowerCase().includes(query),
  );
});

async function scanSoftware(): Promise<void> {
  softwareLoading.value = true;
  errorMsg.value = "";
  try {
    software.value = await junkApi.installedApps();
  } catch (e) {
    errorMsg.value = String(e);
  } finally {
    softwareLoading.value = false;
  }
}

async function uninstallApp(app: (typeof software.value)[number]): Promise<void> {
  if (!app.uninstallCommand) {
    errorMsg.value = t("cleaner.noUninstaller");
    return;
  }
  uninstalling.value = app.id;
  errorMsg.value = "";
  try {
    await junkApi.launchUninstaller(app.uninstallCommand);
    setMessage(t("cleaner.uninstallerStarted", app.name));
    // 卸载由厂商程序交互完成;卸载后扫描安装目录和用户数据目录找残留。
    if (app.installLocation) leftoverRoots.value = [app.installLocation];
  } catch (e) {
    errorMsg.value = String(e);
  } finally {
    uninstalling.value = null;
  }
}

async function scanLeftovers(): Promise<void> {
  const paths = leftoverRoots.value.length ? leftoverRoots.value : selectedRoots.value;
  if (!paths.length) return;
  leftoverScanning.value = true;
  junkScanMode.value = "leftover";
  leftoverItems.value = [];
  selectedLeftovers.value = [];
  leftoverTick.value = 0;
  try {
    await junkApi.scanJunk(paths);
  } catch (e) {
    errorMsg.value = String(e);
    leftoverScanning.value = false;
  }
}

function toggleLeftover(path: string): void {
  const selected = new Set(selectedLeftovers.value);
  if (selected.has(path)) selected.delete(path);
  else selected.add(path);
  selectedLeftovers.value = [...selected];
}

async function handleRowAction(action: RowAction): Promise<void> {
  if (action.type === "copy") {
    try {
      await writeText(action.path);
    } catch {}
    return;
  }
  if (action.type === "reveal") {
    try {
      if (action.isDir) await openPath(action.path);
      else await revealItemInDir(action.path);
    } catch {}
    return;
  }
  if (action.type === "delete") toggleItem(action.path);
}
</script>

<template>
  <div class="flex min-h-0 flex-1 flex-col">
    <header class="panel mx-3 mt-3 flex shrink-0 items-center gap-2 px-3 py-2">
      <div class="brand-logo">
        <img :src="logoUrl" alt="SpaceView" class="size-full" />
      </div>
      <AppNav />
      <div class="ml-auto flex gap-1">
        <Button size="xs" :variant="page === 'junk' ? 'secondary' : 'ghost'" @click="page = 'junk'">
          {{ t("cleaner.junkTab") }}
        </Button>
        <Button size="xs" :variant="page === 'software' ? 'secondary' : 'ghost'" @click="page = 'software'">
          <PackageOpenIcon />
          {{ t("cleaner.softwareTab") }}
        </Button>
      </div>
    </header>

    <!-- 页签切换与路由切换共用同一套 pageEnter / pageLeave 动效 -->
    <Transition mode="out-in" :css="false" @enter="pageEnter" @leave="pageLeave">
      <main v-if="page === 'junk'" key="junk" class="mx-3 mt-2 mb-2 flex min-h-0 flex-1 gap-3 overflow-hidden">
      <!-- 左侧:扫描位置 -->
      <aside class="panel flex w-[360px] shrink-0 flex-col overflow-hidden">
        <div class="flex shrink-0 items-center gap-2 border-b border-border px-3 py-2">
          <span class="truncate text-xs font-semibold">{{ t("cleaner.title") }}</span>
        </div>
        <p class="px-3 py-2 text-[11px] text-muted-foreground">
          {{ t("cleaner.subtitle") }}
        </p>

        <div class="custom-scroll min-h-0 flex-1 overflow-auto px-2 py-2">
          <div v-if="!roots.length" class="px-2 text-xs text-muted-foreground">
            {{ t("cleaner.noRoots") }}
          </div>
          <div v-else>
            <div class="flex items-start gap-1 mb-1">
              <Checkbox
                :model-value="allRootsSelected"
                @click.stop
                @update:model-value="
                  (v) => (v ? (selectedRoots = roots.map((r) => r.path)) : (selectedRoots = []))
                "
              />
              <span class="text-xs font-medium">{{ t("cleaner.all") }}</span>
            </div>
            <div class="grid gap-1">
              <label
                v-for="root in roots"
                :key="root.path"
                class="flex items-center gap-2 px-2 py-1 rounded hover:bg-muted cursor-pointer"
              >
                <Checkbox
                  :model-value="selectedRoots.includes(root.path)"
                  @click.stop
                  @update:model-value="(_v) => toggleRoot(root.path)"
                />
                <span class="truncate text-xs">{{ root.label }}</span>
              </label>
            </div>
          </div>
        </div>

        <div class="flex shrink-0 items-center gap-2 border-t border-border px-3 py-2">
          <Button
            size="xs"
            variant="outline"
            :disabled="scanning"
            @click="cancelScan()"
          >
            {{ t("cleaner.rescan") }}
          </Button>
          <Button size="xs" :disabled="scanning" @click="startScan()">
            {{ scanning ? t("cleaner.scanning") : t("cleaner.scan") }}
          </Button>
        </div>

        <p v-if="errorMsg" class="px-3 py-2 text-xs text-destructive break-words">
          {{ errorMsg }}
        </p>
      </aside>

      <!-- 右侧:结果列表 -->
      <main class="panel flex min-w-0 flex-1 flex-col overflow-hidden">
        <header class="flex shrink-0 items-center gap-2 border-b border-border px-3 py-2">
          <span class="text-xs font-semibold">
            {{ t("cleaner.found", items.length) }}
          </span>
          <div class="ml-auto flex gap-1">
            <Button
              v-for="(k) in ['all', 'temp', 'cache', 'leftover', 'large', 'log']"
              :key="k"
              size="xs"
              variant="outline"
              :class="{
                'bg-brand text-brand-foreground': filterKind === k,
              }"
              @click="filterKind = k"
            >
              {{ KIND_LABEL[k] }}
            </Button>
          </div>
        </header>

        <!-- 列头:与数据行共用 .junk-row 列宽,保证垂直对齐 -->
        <div
          v-if="visibleItems.length"
          class="junk-row junk-head grid shrink-0 items-center gap-3 border-b border-border px-4 pb-1 pt-1"
        >
          <span class="junk-cb"></span>
          <span class="junk-kind"></span>
          <span>{{ t("cleaner.colName") }}</span>
          <span class="junk-reason">{{ t("cleaner.colReason") }}</span>
          <span class="text-right">{{ t("cleaner.colSize") }}</span>
        </div>

        <div class="custom-scroll min-h-0 flex-1 overflow-auto px-2 py-2">
          <template v-if="visibleItems.length === 0">
            <ScanLoading
              v-if="scanning"
              class="px-3"
              compact
              :title="t('cleaner.scanning')"
              :files="scanned"
            />
            <p v-else-if="items.length === 0" class="px-3 py-8 text-center text-xs text-muted-foreground">
              {{ t("cleaner.empty") }}
            </p>
            <p
              v-else
              class="px-3 py-2 text-center text-xs text-muted-foreground"
            >
              已显示前 {RENDER_LIMIT} 项,过滤后剩余
              {filteredItems.length - RENDER_LIMIT} 项
            </p>
          </template>

          <RowContextMenu
            v-for="(it, _i) in visibleItems"
            :key="it.path"
            :path="it.path"
            :is-dir="it.isDir"
            @action="handleRowAction"
          >
            <div
              class="selectable-row junk-row grid cursor-pointer items-center gap-3 rounded-md px-2 py-1.5"
              :title="it.path"
              @dblclick="handleRowAction({ type: 'reveal', path: it.path, isDir: it.isDir })"
            >
              <Checkbox
                :model-value="checked.includes(it.path)"
                @click.stop
                @update:model-value="(_v) => toggleItem(it.path)"
              />
              <Badge :variant="kindVariant(it.kind)" class="junk-kind justify-center">
                {{ KIND_LABEL[it.kind] }}
              </Badge>
              <div class="min-w-0">
                <div class="truncate text-xs font-medium leading-5">{{ fileName(it.path) }}</div>
                <div class="junk-path truncate text-[10px] leading-4 text-muted-foreground">
                  {{ parentDir(it.path) }}
                </div>
              </div>
              <span class="junk-reason truncate text-[10px] text-muted-foreground">
                {{ it.reason }}
              </span>
              <span class="num text-right text-[11px] font-medium leading-5">
                {{ humanSize(it.size) }}
              </span>
            </div>
          </RowContextMenu>
        </div>

        <footer v-if="items.length" class="flex shrink-0 items-center justify-between border-t border-border px-3 py-2">
          <span class="text-xs text-muted-foreground">
            {{ t("cleaner.total", filteredItems.length, humanSize(totalBytes)) }}
          </span>
          <div class="flex gap-1">
            <Button
              size="xs"
              variant="outline"
              :disabled="checked.length === filteredItems.length"
              @click="selectAllItems()"
            >
              {{ t("cleaner.selectAll") }}
            </Button>
            <Button
              size="xs"
              variant="outline"
              :disabled="checked.length === 0"
              @click="clearItems()"
            >
              {{ t("cleaner.clear") }}
            </Button>
            <Button
              size="xs"
              variant="destructive"
              :disabled="checked.length === 0"
              @click="confirmOpen = true"
            >
              <Trash2Icon />
              {{ t("cleaner.delete") }}
            </Button>
          </div>
        </footer>
        <div v-if="deleting" class="border-t border-border px-3 py-2">
          <div class="mb-1 flex items-center gap-2 text-xs text-muted-foreground">
            <LoaderCircleIcon class="size-3.5 animate-spin text-brand" />
            <span>{{ t("cleaner.deleting") }}</span>
            <span class="num ml-auto">{{ deleting.done }}/{{ deleting.total }}</span>
            <span v-if="deleting.current" class="max-w-[35%] truncate">{{ deleting.current }}</span>
          </div>
          <div class="h-1.5 overflow-hidden rounded-full bg-muted">
            <div class="h-full bg-brand transition-all" :style="{ width: `${deleting.total ? deleting.done / deleting.total * 100 : 0}%` }"></div>
          </div>
        </div>
      </main>
    </main>

    <main v-else key="software" class="mx-3 mt-2 mb-2 flex min-h-0 flex-1 flex-col gap-3 overflow-hidden">
      <section class="panel flex min-h-0 flex-1 flex-col overflow-hidden">
        <header class="flex shrink-0 items-center gap-2 border-b border-border px-3 py-2">
          <PackageOpenIcon class="size-4 text-brand" />
          <div>
            <h2 class="text-sm font-semibold">{{ t("cleaner.softwareTitle") }}</h2>
            <p class="text-[11px] text-muted-foreground">{{ t("cleaner.softwareSubtitle") }}</p>
          </div>
          <Button class="ml-auto" size="sm" :disabled="softwareLoading" @click="scanSoftware">
            <LoaderCircleIcon v-if="softwareLoading" class="animate-spin" />
            <SearchIcon v-else />
            {{ softwareLoading ? t("cleaner.scanning") : t("cleaner.scanSoftware") }}
          </Button>
        </header>
        <div class="flex shrink-0 items-center gap-2 border-b border-border px-3 py-2">
          <SearchIcon class="size-3.5 text-muted-foreground" />
          <input v-model="softwareSearch" class="h-8 min-w-0 flex-1 bg-transparent text-xs outline-none" :placeholder="t('cleaner.softwareSearch')" />
          <span class="text-[11px] text-muted-foreground">{{ software.length }}</span>
        </div>
        <div class="custom-scroll min-h-0 flex-1 overflow-auto p-2">
          <div v-if="!software.length" class="grid h-full place-items-center text-xs text-muted-foreground">
            {{ t("cleaner.softwareEmpty") }}
          </div>
          <div v-else class="grid gap-1">
            <div v-for="app in filteredSoftware" :key="app.id" class="software-row grid grid-cols-[minmax(0,1fr)_150px_100px_112px] items-center gap-3 rounded-lg border border-transparent px-3 py-2 hover:border-border hover:bg-muted/50">
              <div class="min-w-0">
                <div class="truncate text-xs font-medium">{{ app.name }}</div>
                <div class="truncate text-[10px] text-muted-foreground">{{ app.publisher || t("cleaner.unknownPublisher") }}</div>
              </div>
              <div class="truncate text-[10px] text-muted-foreground">{{ app.version || "-" }}</div>
              <div class="text-right text-[10px] text-muted-foreground">{{ app.estimatedSize ? humanSize(app.estimatedSize) : "-" }}</div>
              <Button size="xs" variant="outline" :disabled="uninstalling === app.id || !app.uninstallCommand" @click="uninstallApp(app)">
                <LoaderCircleIcon v-if="uninstalling === app.id" class="animate-spin" />
                {{ uninstalling === app.id ? t("cleaner.uninstalling") : t("cleaner.uninstall") }}
              </Button>
            </div>
          </div>
        </div>
      </section>

      <section class="panel flex min-h-0 flex-[0.7] flex-col overflow-hidden">
        <header class="flex shrink-0 items-center gap-2 border-b border-border px-3 py-2">
          <span class="text-xs font-semibold">{{ t("cleaner.leftoverTitle") }}</span>
          <span class="text-[11px] text-muted-foreground">{{ leftoverItems.length }}</span>
          <Button class="ml-auto" size="xs" :disabled="leftoverScanning" @click="scanLeftovers">
            <LoaderCircleIcon v-if="leftoverScanning" class="animate-spin" />
            {{ leftoverScanning ? t("cleaner.scanning") : t("cleaner.scanLeftovers") }}
          </Button>
        </header>
        <div class="custom-scroll min-h-0 flex-1 overflow-auto p-2">
          <div v-if="!leftoverItems.length" class="grid h-full place-items-center text-xs text-muted-foreground">
            {{ t("cleaner.leftoverEmpty") }}
          </div>
          <div v-else class="grid gap-1">
            <div v-for="item in leftoverItems.slice(0, RENDER_LIMIT)" :key="item.path" class="grid grid-cols-[24px_minmax(0,1fr)_100px] items-center gap-2 rounded px-2 py-1 hover:bg-muted/50">
              <Checkbox :model-value="selectedLeftovers.includes(item.path)" @update:model-value="toggleLeftover(item.path)" />
              <span class="truncate text-xs" :title="item.path">{{ item.path }}</span>
              <span class="text-right text-[10px] text-muted-foreground">{{ humanSize(item.size) }}</span>
            </div>
          </div>
        </div>
      </section>
    </main>
    </Transition>

    <ConfirmDelete
      :open="confirmOpen"
      :paths="checked"
      @update:open="confirmOpen = $event"
      @confirm="performDelete"
    />
  </div>
</template>