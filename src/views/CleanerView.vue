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
import { Trash2Icon } from "@lucide/vue";

import { api, junkApi, bindJunkEvents } from "@/api";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Badge } from "@/components/ui/badge";
import logoUrl from "@/assets/logo.png";
import AppNav from "@/components/AppNav.vue";
import { t } from "@/i18n";
import { humanSize, shortPath } from "@/lib/format";
import type { JunkItem, JunkRoot, RowAction } from "@/types";

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

let unlisteners: UnlistenFn[] = [];

const KIND_LABEL: Record<string, string> = {
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
  unlisteners = await bindJunkEvents({
    onTick: (p: { scanned: number }) => {
      scanned.value = p.scanned;
    },
    onDone: (p: { items: JunkItem[] }) => {
      items.value = p.items;
      scanning.value = false;
    },
    onFailed: (m: string) => {
      scanning.value = false;
      errorMsg.value = m;
    },
  });
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
  if (!paths.length) return;
  try {
    await api.deletePaths(paths);
  } catch (e) {
    errorMsg.value = String(e);
    return;
  }
  checked.value = [];
  items.value = items.value.filter((i) => !paths.includes(i.path));
  if (items.value.length === 0) errorMsg.value = "";
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
    </header>

    <main class="mx-3 mt-2 mb-2 flex min-h-0 flex-1 gap-3 overflow-hidden">
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
              v-for="(k) in ['temp', 'cache', 'leftover', 'large', 'log']"
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

        <div class="custom-scroll min-h-0 flex-1 overflow-auto px-2 py-2">
          <template v-if="visibleItems.length === 0">
            <p v-if="scanning" class="px-3 py-8 text-center text-xs text-muted-foreground">
              {{ t("cleaner.scanning") }}
            </p>
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
              class="selectable-row flex cursor-pointer items-center gap-2 rounded-md px-2 py-1"
              :title="it.path"
              @dblclick="handleRowAction({ type: 'reveal', path: it.path, isDir: it.isDir })"
            >
              <Checkbox
                :model-value="checked.includes(it.path)"
                @click.stop
                @update:model-value="(_v) => toggleItem(it.path)"
              />
              <Badge :variant="kindVariant(it.kind)">
                {{ KIND_LABEL[it.kind] }}
              </Badge>
              <span class="truncate text-xs font-medium">
                {{ it.path.split(/[\\/]+/).pop() }}
              </span>
              <span class="num ml-auto w-24 shrink-0 text-right text-[10px] text-muted-foreground">
                {{ humanSize(it.size) }}
              </span>
              <span class="ml-auto shrink-0 text-[10px] text-muted-foreground hidden md:inline">
                {{ shortPath(it.path, 38) }}
              </span>
              <span class="text-xs text-muted-foreground" v-if="it.reason">{{ it.reason }}</span>
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
      </main>
    </main>

    <ConfirmDelete
      :open="confirmOpen"
      :paths="checked"
      @update:open="confirmOpen = $event"
      @confirm="performDelete"
    />
  </div>
</template>