<script setup lang="ts">
/** Treemap 视图:把当前目录的子目录 + 直属文件按 squarify 布局成矩形块。
 *  左键下钻、面包屑返回、悬停显示占比详情、右键菜单复用 RowContextMenu。
 *  块位置用 CSS transition 补间,扫描快照刷新时自然形变(对齐旧版动画)。 */
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { ArrowLeftIcon } from "@lucide/vue";

import { Button } from "@/components/ui/button";
import { t } from "@/i18n";
import { dirPath, fitWidth, formatNumber, humanSize } from "@/lib/format";
import {
  blockColor,
  blockTextColor,
  collectBlocks,
  squarify,
  type Rect,
  type TreemapBlock,
} from "@/lib/treemap";
import type { RowAction, Tree } from "@/types";

import RowContextMenu from "./RowContextMenu.vue";

const props = defineProps<{
  tree: Tree;
  drill: number[];
  epoch: number;
  dark: boolean;
}>();

const emit = defineEmits<{
  (e: "drill", idx: number): void;
  /** 回到面包屑的某一层(层级下标) */
  (e: "back", level: number): void;
  (e: "action", action: RowAction): void;
}>();

const canvas = ref<HTMLElement | null>(null);
const size = ref({ w: 0, h: 0 });
let observer: ResizeObserver | null = null;

onMounted(() => {
  if (!canvas.value) return;
  observer = new ResizeObserver((entries) => {
    const box = entries[0]?.contentRect;
    if (box) size.value = { w: box.width, h: box.height };
  });
  observer.observe(canvas.value);
});
onBeforeUnmount(() => observer?.disconnect());

const current = computed(() => props.drill[props.drill.length - 1] ?? 0);
const blockSet = computed(() => collectBlocks(props.tree, current.value));

/** squarify 返回 [blockIndex, size, rect],按权重降序,需按索引回查块 */
const laid = computed(() =>
  squarify(
    blockSet.value.blocks.map((b, i) => [i, b.size] as [number, number]),
    { x: 0, y: 0, w: size.value.w, h: size.value.h } as Rect,
  ),
);

function blockOf(n: number): TreemapBlock {
  return blockSet.value.blocks[laid.value[n][0]];
}

function blockKey(n: number): string {
  const b = blockOf(n);
  return `${props.epoch}-${b.kind}-${b.idx}`;
}

function blockName(n: number): string {
  const b = blockOf(n);
  if (b.kind === "files") return t("tm.own");
  if (b.kind === "other") return t("tm.other", b.merged);
  const node = props.tree.nodes[b.idx];
  return b.deleted ? `${node.name} · ${t("tm.deleted")}` : node.name;
}

function crumbName(idx: number): string {
  return idx === 0 ? props.tree.rootPath : props.tree.nodes[idx].name;
}

function colorOf(n: number): string {
  const b = blockOf(n);
  if (b.deleted) return props.dark ? "#4b4f5a" : "#cbd0d8";
  return blockColor(laid.value[n][0], b.kind, props.dark);
}

function isDirBlock(n: number): boolean {
  const b = blockOf(n);
  return b.kind === "dir" && !b.deleted;
}

function blockPath(n: number): string {
  const b = blockOf(n);
  return b.kind === "dir" ? dirPath(props.tree, b.idx) : dirPath(props.tree, current.value);
}

function blockStyle(n: number, r: Rect) {
  return {
    left: `${r.x}px`,
    top: `${r.y}px`,
    width: `${Math.max(1, r.w - 1)}px`,
    height: `${Math.max(1, r.h - 1)}px`,
    backgroundColor: colorOf(n),
  };
}

function onBlockClick(n: number): void {
  if (!isDirBlock(n)) return;
  emit("drill", blockOf(n).idx);
}

interface HoverState {
  x: number;
  y: number;
  text: string;
}
const hover = ref<HoverState | null>(null);

function onHover(e: MouseEvent, n: number): void {
  const b = blockOf(n);
  const fileCount =
    b.kind === "dir"
      ? props.tree.nodes[b.idx].fileCount
      : props.tree.nodes[current.value].fileCount;
  const pct =
    blockSet.value.curSize > 0
      ? ((b.size / blockSet.value.curSize) * 100).toFixed(1)
      : "0.0";
  hover.value = {
    x: e.clientX + 14,
    y: e.clientY + 14,
    text: `${blockName(n)}\n${blockPath(n)}\n${humanSize(b.size)}  ${t("tm.files", formatNumber(fileCount))}  · ${t("tm.pct", pct)}`,
  };
}
</script>

<template>
  <div class="flex h-full min-h-0 flex-col">
    <!-- 面包屑 + 当前视图摘要 -->
    <div class="flex shrink-0 items-center gap-2 pb-2">
      <Button
        variant="ghost"
        size="icon-xs"
        :disabled="props.drill.length <= 1"
        :title="t('tm.back')"
        @click="emit('back', props.drill.length - 2)"
      >
        <ArrowLeftIcon />
      </Button>
      <div class="custom-scroll flex min-w-0 items-center gap-1 overflow-x-auto whitespace-nowrap">
        <template v-for="(idx, lvl) in props.drill" :key="lvl">
          <button
            class="rounded px-1 py-0.5 text-xs hover:bg-muted disabled:font-semibold disabled:text-foreground disabled:opacity-100"
            :disabled="lvl === props.drill.length - 1"
            @click="emit('back', lvl)"
          >
            {{ crumbName(idx) }}
          </button>
          <span v-if="lvl < props.drill.length - 1" class="text-muted-foreground">›</span>
        </template>
      </div>
      <span class="num ml-auto shrink-0 text-[10px] text-muted-foreground">
        {{ t("tm.current", crumbName(current), humanSize(blockSet.curSize)) }}
      </span>
    </div>

    <div ref="canvas" class="treemap-canvas min-h-0 flex-1 rounded-lg">
      <template v-if="laid.length">
        <RowContextMenu
          v-for="(item, n) in laid"
          :key="blockKey(n)"
          :path="blockPath(n)"
          :is-dir="true"
          :disabled="!isDirBlock(n)"
          @action="emit('action', $event)"
        >
          <div
            class="treemap-block"
            :class="{
              'is-deleted': blockOf(n).deleted,
              'is-static': !isDirBlock(n),
            }"
            :style="blockStyle(n, item[2])"
            @click="onBlockClick(n)"
            @mousemove="onHover($event, n)"
            @mouseleave="hover = null"
          >
            <div
              v-if="item[2].w > 62 && item[2].h > 16"
              class="flex h-full w-full flex-col items-center justify-center gap-0.5 px-1 text-center"
              :style="{ color: blockTextColor(colorOf(n)) }"
            >
              <span class="max-w-full truncate text-[11px] leading-tight font-medium">
                {{ fitWidth(blockName(n), item[2].w - 8, 11) }}
              </span>
              <span v-if="item[2].h > 34" class="num text-[10px] leading-tight opacity-80">
                {{ humanSize(blockOf(n).size) }}
              </span>
            </div>
          </div>
        </RowContextMenu>
      </template>
      <div v-else class="grid h-full place-items-center text-xs text-muted-foreground">
        {{ t("tm.empty") }}
      </div>
    </div>

    <!-- 悬停详情(固定定位,跟随鼠标) -->
    <div
      v-if="hover"
      class="tm-tooltip"
      :style="{ left: `${hover.x}px`, top: `${hover.y}px` }"
    >
      {{ hover.text }}
    </div>
  </div>
</template>
