<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { CircleAlertIcon, LoaderCircleIcon, PauseIcon } from "@lucide/vue";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Progress as ProgressBar } from "@/components/ui/progress";
import { t } from "@/i18n";
import { formatNumber, humanSize, shortPath } from "@/lib/format";
import type {
  AppPhase,
  DeleteJobState,
  Progress as ProgressData,
  ScanResult,
} from "@/types";

const props = defineProps<{
  phase: AppPhase;
  paused: boolean;
  progress: ProgressData;
  result: ScanResult | null;
  error: string;
  deleting: DeleteJobState | null;
}>();

const emit = defineEmits<{
  (e: "cancel-delete"): void;
}>();

const now = ref(performance.now());
let timer: number | undefined;
onMounted(() => {
  timer = window.setInterval(() => {
    now.value = performance.now();
  }, 200);
});
onBeforeUnmount(() => {
  if (timer !== undefined) window.clearInterval(timer);
});

/** 有效扫描耗时:总流逝时间减去全部暂停区间 */
const elapsed = computed(() => {
  if (props.progress.startedAt === null) return 0;
  let e = (now.value - props.progress.startedAt) / 1000 - props.progress.pausedSecs;
  if (props.progress.pausedAt !== null) {
    e -= (now.value - props.progress.pausedAt) / 1000;
  }
  return Math.max(0, e);
});

const speed = computed(() => {
  const secs = Math.max(0.5, elapsed.value);
  return {
    files: props.progress.files / secs,
    mb: props.progress.bytes / secs / 1_048_576,
  };
});
</script>

<template>
  <footer class="panel flex min-h-10 shrink-0 items-center gap-2 px-3 py-1.5 text-xs">
    <!-- 后台删除进度优先占用状态栏 -->
    <template v-if="props.deleting">
      <LoaderCircleIcon class="size-3.5 shrink-0 animate-spin text-brand" />
      <span class="shrink-0">
        {{ props.deleting.cancelled ? t("status.cancelling") : t("status.deleting") }}
        <span class="num">{{ props.deleting.done }}/{{ props.deleting.total }}</span>
      </span>
      <span
        class="num max-w-[36%] truncate text-muted-foreground"
        :title="props.deleting.current ?? ''"
      >
        {{ props.deleting.current ? shortPath(props.deleting.current) : "" }}
      </span>
      <ProgressBar
        :model-value="
          props.deleting.total ? (props.deleting.done / props.deleting.total) * 100 : 100
        "
        class="ml-1 w-40"
      />
      <Button
        variant="outline"
        size="xs"
        class="ml-auto"
        :disabled="props.deleting.cancelled"
        @click="emit('cancel-delete')"
      >
        {{ t("status.cancelRemaining") }}
      </Button>
    </template>

    <template v-else-if="props.phase === 'scanning'">
      <PauseIcon v-if="props.paused" class="size-3.5 shrink-0 text-amber-500" />
      <LoaderCircleIcon v-else class="size-3.5 shrink-0 animate-spin text-brand" />
      <Badge v-if="props.paused" variant="outline" class="shrink-0">
        {{ t("status.paused") }}
      </Badge>
      <span class="num shrink-0">
        {{
          t(
            "status.scanning",
            formatNumber(props.progress.dirs),
            formatNumber(props.progress.files),
            humanSize(props.progress.bytes),
          )
        }}
      </span>
      <span class="num shrink-0 text-muted-foreground">
        {{
          t(
            "status.speed",
            elapsed.toFixed(1),
            Math.round(speed.files),
            speed.mb.toFixed(1),
          )
        }}
      </span>
      <span
        class="num ml-auto max-w-[42%] truncate text-muted-foreground"
        :title="props.progress.cur"
      >
        {{ t("status.current", shortPath(props.progress.cur)) }}
      </span>
    </template>

    <template v-else-if="props.phase === 'done' && props.result">
      <span class="size-1.5 shrink-0 rounded-full bg-emerald-500" />
      <span class="num">
        {{
          t(
            "status.done",
            formatNumber(props.result.totalFiles),
            humanSize(props.result.totalBytes),
            props.result.denied.length,
          )
        }}
      </span>
    </template>

    <template v-else-if="props.phase === 'failed'">
      <CircleAlertIcon class="size-3.5 shrink-0 text-destructive" />
      <span class="text-destructive">{{ t("status.failed", props.error) }}</span>
    </template>

    <template v-else>
      <span class="text-muted-foreground">{{ t("status.ready") }}</span>
    </template>
  </footer>
</template>
