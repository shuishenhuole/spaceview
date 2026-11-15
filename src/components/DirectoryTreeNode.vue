<script setup lang="ts">
import { computed } from "vue";
import { ChevronRightIcon } from "@lucide/vue";

import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
import { t } from "@/i18n";
import { dirPath, humanSize, percent } from "@/lib/format";
import type { RowAction, Tree } from "@/types";

import DirectoryTreeNode from "./DirectoryTreeNode.vue";
import RowContextMenu from "./RowContextMenu.vue";

defineOptions({ name: "DirectoryTreeNode" });

const props = defineProps<{
  tree: Tree;
  idx: number;
  rootSize: number;
  depth: number;
  expanded: Set<number>;
  maxChildren: number;
}>();

const emit = defineEmits<{
  (e: "navigate", idx: number): void;
  (e: "toggle", idx: number): void;
  (e: "action", action: RowAction): void;
}>();

const node = computed(() => props.tree.nodes[props.idx]);
const path = computed(() => dirPath(props.tree, props.idx));
const kids = computed(() =>
  node.value.children
    .slice()
    .sort((a, b) => props.tree.nodes[b].size - props.tree.nodes[a].size)
    .slice(0, props.maxChildren),
);
const hidden = computed(() => Math.max(0, node.value.children.length - props.maxChildren));
const hasChildren = computed(() => node.value.children.length > 0 && !node.value.deleted);
const isOpen = computed(() => props.expanded.has(props.idx));
</script>

<template>
  <div class="min-w-0">
    <RowContextMenu
      :path="path"
      :is-dir="true"
      :disabled="node.deleted"
      @action="emit('action', $event)"
    >
      <div
        class="selectable-row flex cursor-pointer items-center gap-1 rounded-md px-1.5 py-1"
        :style="{ paddingLeft: `${depth * 12 + 6}px` }"
        :title="path"
        @click="emit('navigate', idx)"
      >
        <button
          v-if="hasChildren"
          class="grid size-3.5 shrink-0 place-items-center rounded text-muted-foreground hover:bg-muted"
          @click.stop="emit('toggle', idx)"
        >
          <ChevronRightIcon
            class="size-3 transition-transform duration-150"
            :class="isOpen ? 'rotate-90' : ''"
          />
        </button>
        <span v-else class="w-3.5 shrink-0" />

        <span class="truncate text-xs font-medium">{{ node.name }}</span>
        <Badge v-if="node.deleted" variant="outline" class="ml-1 shrink-0 px-1 py-0 text-[9px]">
          {{ t("tree.deleted") }}
        </Badge>
        <Badge v-else-if="node.denied" variant="secondary" class="ml-1 shrink-0 px-1 py-0 text-[9px]">
          {{ t("tree.restricted") }}
        </Badge>
        <span class="num ml-auto shrink-0 pl-2 text-[10px] text-muted-foreground">
          {{ humanSize(node.size) }}
        </span>
      </div>
    </RowContextMenu>

    <Progress :model-value="percent(node.size, rootSize)" class="mx-1.5 h-[3px]" />

    <div v-if="isOpen && hasChildren">
      <DirectoryTreeNode
        v-for="c in kids"
        :key="c"
        :tree="tree"
        :idx="c"
        :root-size="rootSize"
        :depth="depth + 1"
        :expanded="expanded"
        :max-children="maxChildren"
        @navigate="emit('navigate', $event)"
        @toggle="emit('toggle', $event)"
        @action="emit('action', $event)"
      />
      <p v-if="hidden > 0" class="px-3 py-1 text-[10px] text-muted-foreground">
        {{ t("tree.smaller", hidden) }}
      </p>
    </div>
  </div>
</template>
