<script setup lang="ts">
/** 操作反馈气泡:入场/离场由 GSAP 通过 Vue <Transition> 的 JS 钩子控制,
 *  "减少动画"时 motion.ts 会直接落到终态。 */
import { CheckIcon } from "@lucide/vue";

import { enterToast, leaveToast } from "@/lib/motion";

const props = defineProps<{
  message: string | null;
}>();
</script>

<template>
  <Transition mode="out-in" :css="false" @enter="enterToast" @leave="leaveToast">
    <div
      v-if="props.message"
      :key="props.message"
      class="pointer-events-none fixed right-4 bottom-14 z-50 max-w-md rounded-xl border border-border bg-popover/95 px-4 py-2.5 text-xs text-popover-foreground shadow-xl backdrop-blur"
    >
      <div class="flex items-start gap-2">
        <CheckIcon class="mt-0.5 size-3.5 shrink-0 text-emerald-500" />
        <span class="break-words">{{ props.message }}</span>
      </div>
    </div>
  </Transition>
</template>
