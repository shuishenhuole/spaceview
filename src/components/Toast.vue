<script setup lang="ts">
import { CheckIcon } from "@lucide/vue";

const props = defineProps<{
  message: string | null;
}>();
</script>

<template>
  <div
    v-if="props.message"
    :key="props.message"
    class="toast-box pointer-events-none fixed right-4 bottom-14 z-50 max-w-md rounded-xl border border-border bg-popover/95 px-4 py-2.5 text-xs text-popover-foreground shadow-xl backdrop-blur"
  >
    <div class="flex items-start gap-2">
      <CheckIcon class="mt-0.5 size-3.5 shrink-0 text-emerald-500" />
      <span class="break-words">{{ props.message }}</span>
    </div>
  </div>
</template>
