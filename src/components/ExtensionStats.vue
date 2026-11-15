<script setup lang="ts">
import { computed } from "vue";

import { Progress } from "@/components/ui/progress";
import { t } from "@/i18n";
import { extLabel, formatNumber, humanSize, percent } from "@/lib/format";
import type { ScanResult } from "@/types";

const props = defineProps<{
  result: ScanResult;
}>();

const rows = computed(() => props.result.extStats.slice(0, 40));
const total = computed(() => Math.max(1, props.result.tree.nodes[0]?.size ?? 1));
</script>

<template>
  <div class="flex h-full min-h-0 flex-col">
    <p class="mb-3 shrink-0 text-xs text-muted-foreground">
      {{ t("ext.title") }}
    </p>

    <div class="custom-scroll min-h-0 flex-1 space-y-3 overflow-auto pr-2">
      <div v-for="stat in rows" :key="stat.ext" class="space-y-1.5">
        <div class="flex items-baseline justify-between gap-3 text-xs">
          <span class="truncate font-medium">{{ extLabel(stat.ext) }}</span>
          <span class="num shrink-0 text-muted-foreground">
            {{ humanSize(stat.bytes) }} · {{ formatNumber(stat.count) }} ·
            {{ percent(stat.bytes, total).toFixed(1) }}%
          </span>
        </div>
        <Progress :model-value="percent(stat.bytes, total)" class="h-1.5" />
      </div>

      <p v-if="props.result.extStats.length > 40" class="pt-1 text-xs text-muted-foreground">
        {{ t("ext.more", props.result.extStats.length - 40) }}
      </p>
    </div>
  </div>
</template>
