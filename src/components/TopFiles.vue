<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { Trash2Icon } from "@lucide/vue";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { t } from "@/i18n";
import { fileName, humanSize, parentDir } from "@/lib/format";
import type { FileHit, RowAction, SortBy } from "@/types";

import RowContextMenu from "./RowContextMenu.vue";

const props = defineProps<{
  rows: FileHit[];
  matched: number;
  renderLimit: number;
  sortBy: SortBy;
  selected: string[];
  focusRow: number | null;
}>();

const emit = defineEmits<{
  (e: "update:sortBy", value: SortBy): void;
  (e: "update:selected", value: string[]): void;
  (e: "update:focusRow", value: number | null): void;
  (e: "action", action: RowAction): void;
  (e: "request-delete"): void;
}>();

const listEl = ref<HTMLElement | null>(null);

const allSelected = computed(
  () => props.rows.length > 0 && props.rows.every((h) => props.selected.includes(h.path)),
);

function toggle(path: string, on: boolean): void {
  const next = props.selected.filter((p) => p !== path);
  if (on) next.push(path);
  emit("update:selected", next);
}

function toggleAll(on: boolean): void {
  const set = new Set(props.selected);
  for (const h of props.rows) {
    if (on) set.add(h.path);
    else set.delete(h.path);
  }
  emit("update:selected", [...set]);
}

function onRowClick(e: MouseEvent, i: number): void {
  // Shift+单击:从旧光标(锚点)到当前行的区间并入勾选集
  if (e.shiftKey && props.focusRow !== null) {
    const a = Math.min(props.focusRow, i);
    const b = Math.max(props.focusRow, i);
    const set = new Set(props.selected);
    for (let k = a; k <= b; k++) {
      if (props.rows[k]) set.add(props.rows[k].path);
    }
    emit("update:selected", [...set]);
  }
  emit("update:focusRow", i);
}

watch(
  () => props.focusRow,
  (f) => {
    if (f === null) return;
    const el = listEl.value?.querySelector(`[data-row="${f}"]`);
    el?.scrollIntoView({ block: "nearest" });
  },
);
</script>

<template>
  <div class="flex h-full min-h-0 flex-col">
    <div class="flex shrink-0 flex-wrap items-center gap-2 pb-1 text-xs">
      <Badge v-if="props.selected.length" variant="secondary">
        {{ t("files.selected", props.selected.length) }}
      </Badge>
      <span class="text-[11px] text-muted-foreground">
        {{ t("files.matched", props.matched, props.renderLimit) }} ·
        {{ t("files.hint") }}
      </span>
      <Button
        v-if="props.selected.length"
        variant="destructive"
        size="xs"
        class="ml-auto"
        @click="emit('request-delete')"
      >
        <Trash2Icon />
        {{ t("files.delete") }}
      </Button>
    </div>

    <div class="flex shrink-0 items-center gap-2 border-b border-border py-1.5">
      <Checkbox
        :model-value="allSelected"
        :aria-label="t('files.all')"
        @update:model-value="(v) => toggleAll(v === true)"
      />
      <Button variant="ghost" size="xs" @click="emit('update:sortBy', 'size')">
        {{ t("files.size") }} {{ props.sortBy === "size" ? "▼" : "" }}
      </Button>
      <Button variant="ghost" size="xs" @click="emit('update:sortBy', 'name')">
        {{ props.sortBy === "name" ? t("files.name") : t("files.nameShort") }}
        {{ props.sortBy === "name" ? "▲" : "" }}
      </Button>
      <span class="ml-auto hidden text-[10px] text-muted-foreground lg:inline">
        {{ t("files.rowHint") }}
      </span>
    </div>

    <div ref="listEl" class="custom-scroll min-h-0 flex-1 overflow-auto">
      <RowContextMenu
        v-for="(hit, i) in props.rows"
        :key="hit.path"
        :path="hit.path"
        :is-dir="false"
        @action="emit('action', $event)"
      >
        <div
          :data-row="i"
          class="selectable-row flex cursor-pointer items-center gap-2 rounded-md px-2 py-1"
          :class="{
            'bg-primary/10 ring-1 ring-brand/30': props.selected.includes(hit.path),
            'bg-accent': props.focusRow === i && !props.selected.includes(hit.path),
          }"
          :title="hit.path"
          @click="onRowClick($event, i)"
          @dblclick="emit('action', { type: 'reveal', path: hit.path, isDir: false })"
        >
          <Checkbox
            :model-value="props.selected.includes(hit.path)"
            @click.stop
            @update:model-value="(v) => toggle(hit.path, v === true)"
          />
          <span class="num w-24 shrink-0 text-right text-xs">
            {{ humanSize(hit.size) }}
          </span>
          <span class="truncate text-xs font-medium">
            {{ fileName(hit.path) }}
          </span>
          <span
            class="ml-auto hidden truncate text-[10px] text-muted-foreground md:inline"
          >
            {{ parentDir(hit.path) }}
          </span>
        </div>
      </RowContextMenu>

      <p v-if="!props.rows.length" class="px-3 py-8 text-center text-xs text-muted-foreground">
        {{ t("files.empty") }}
      </p>
    </div>
  </div>
</template>
