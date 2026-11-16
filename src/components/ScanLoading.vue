<script setup lang="ts">
/** 统一的扫描 loading:Logo 脉冲环 + 旋转指示 + 实时统计。
 *  磁盘分析与电脑清理共用,保证两处观感一致。 */
import { LoaderCircleIcon } from "@lucide/vue";

import logoUrl from "@/assets/logo.png";
import { t } from "@/i18n";
import { formatNumber, humanSize, shortPath } from "@/lib/format";

const props = withDefaults(
  defineProps<{
    title?: string;
    dirs?: number;
    files?: number;
    bytes?: number;
    cur?: string;
    compact?: boolean;
  }>(),
  { title: "", dirs: 0, files: 0, bytes: 0, cur: "", compact: false },
);
</script>

<template>
  <div
    class="flex flex-col items-center justify-center text-center"
    :class="props.compact ? 'gap-2 py-6' : 'gap-4 py-10'"
  >
    <div class="scan-logo-wrap" :class="props.compact ? 'scan-logo-sm' : ''">
      <img :src="logoUrl" alt="" class="scan-logo" />
      <span class="scan-ring"></span>
    </div>

    <div class="flex items-center gap-2 text-sm font-medium">
      <LoaderCircleIcon class="size-4 animate-spin text-brand" />
      {{ props.title || t("scanning.wait") }}
    </div>

    <div v-if="props.files > 0" class="num text-xs text-muted-foreground">
      {{
        t(
          "status.scanning",
          formatNumber(props.dirs),
          formatNumber(props.files),
          humanSize(props.bytes),
        )
      }}
    </div>

    <div
      v-if="props.cur"
      class="max-w-[80%] truncate text-[11px] text-muted-foreground"
      :title="props.cur"
    >
      {{ shortPath(props.cur, 72) }}
    </div>
  </div>
</template>