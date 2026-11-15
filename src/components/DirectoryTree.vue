<script setup lang="ts">
import { computed, ref } from "vue";
import { PanelLeftIcon } from "@lucide/vue";

import { Badge } from "@/components/ui/badge";
import { t } from "@/i18n";
import { dirPath, humanSize } from "@/lib/format";
import type { RowAction, Tree } from "@/types";

import DirectoryTreeNode from "./DirectoryTreeNode.vue";
import RowContextMenu from "./RowContextMenu.vue";

const MAX_CHILDREN_SHOWN = 200;
const FLAT_LIMIT = 500;

const props = defineProps<{
  tree: Tree;
  needle: string;
  minBytes: number;
}>();

const emit = defineEmits<{
  (e: "navigate", idx: number): void;
  (e: "action", action: RowAction): void;
}>();

const expanded = ref<Set<number>>(new Set());
const filterActive = computed(
  () => props.needle.trim().length > 0 || props.minBytes > 0,
);
const rootSize = computed(() => props.tree.nodes[0]?.size ?? 0);
const rootChildren = computed(() =>
  (props.tree.nodes[0]?.children ?? [])
    .slice()
    .sort((a, b) => props.tree.nodes[b].size - props.tree.nodes[a].size)
    .slice(0, MAX_CHILDREN_SHOWN),
);
const rootHidden = computed(() =>
  Math.max(0, (props.tree.nodes[0]?.children.length ?? 0) - MAX_CHILDREN_SHOWN),
);

const flat = computed(() => {
  const needle = props.needle.trim().toLowerCase();
  const rows: number[] = [];
  let total = 0;
  for (let i = 1; i < props.tree.nodes.length; i++) {
    const n = props.tree.nodes[i];
    if (n.size < props.minBytes) continue;
    if (needle && !n.name.toLowerCase().includes(needle)) continue;
    total += 1;
    if (rows.length < FLAT_LIMIT) rows.push(i);
  }
  return { rows, total };
});

function toggle(idx: number): void {
  const next = new Set(expanded.value);
  if (next.has(idx)) next.delete(idx);
  else next.add(idx);
  expanded.value = next;
}
</script>

<template>
  <div class="flex h-full min-h-0 flex-col">
    <div class="flex shrink-0 items-center gap-2 border-b border-border px-3 py-2">
      <PanelLeftIcon class="size-3.5 text-brand" />
      <span class="truncate text-xs font-semibold">
        {{
          filterActive
            ? t("tree.search", needle.trim() || "-")
            : t("tree.title")
        }}
      </span>
    </div>

    <div class="custom-scroll min-h-0 flex-1 overflow-auto py-1.5">
      <!-- 过滤态:扁平命中列表 -->
      <template v-if="filterActive">
        <RowContextMenu
          v-for="i in flat.rows"
          :key="i"
          :path="dirPath(tree, i)"
          :is-dir="true"
          :disabled="tree.nodes[i].deleted"
          @action="emit('action', $event)"
        >
          <div
            class="selectable-row flex cursor-pointer items-center gap-2 px-3 py-1 text-xs"
            :title="dirPath(tree, i)"
            @click="!tree.nodes[i].deleted && emit('navigate', i)"
          >
            <span class="truncate">{{ tree.nodes[i].name }}</span>
            <Badge
              v-if="tree.nodes[i].deleted"
              variant="outline"
              class="shrink-0 px-1 py-0 text-[9px]"
            >
              {{ t("tree.deleted") }}
            </Badge>
            <Badge
              v-else-if="tree.nodes[i].denied"
              variant="secondary"
              class="shrink-0 px-1 py-0 text-[9px]"
            >
              {{ t("tree.restricted") }}
            </Badge>
            <span class="num ml-auto shrink-0 text-[10px] text-muted-foreground">
              {{ humanSize(tree.nodes[i].size) }}
            </span>
          </div>
        </RowContextMenu>

        <p v-if="flat.total === 0" class="px-3 py-4 text-xs text-muted-foreground">
          {{ t("tree.empty") }}
        </p>
        <p
          v-else-if="flat.total > flat.rows.length"
          class="px-3 py-2 text-[10px] text-muted-foreground"
        >
          {{ t("tree.more", flat.total, flat.rows.length) }}
        </p>
      </template>

      <!-- 正常态:递归目录树 -->
      <template v-else>
        <DirectoryTreeNode
          v-for="c in rootChildren"
          :key="c"
          :tree="tree"
          :idx="c"
          :root-size="rootSize"
          :depth="0"
          :expanded="expanded"
          :max-children="MAX_CHILDREN_SHOWN"
          @navigate="emit('navigate', $event)"
          @toggle="toggle"
          @action="emit('action', $event)"
        />
        <p v-if="rootHidden > 0" class="px-3 py-1 text-[10px] text-muted-foreground">
          {{ t("tree.smaller", rootHidden) }}
        </p>
      </template>
    </div>
  </div>
</template>
