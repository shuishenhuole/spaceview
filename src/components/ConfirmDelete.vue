<script setup lang="ts">
import { AlertTriangleIcon } from "@lucide/vue";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { t } from "@/i18n";

const props = defineProps<{
  open: boolean;
  paths: string[];
}>();

const emit = defineEmits<{
  (e: "update:open", value: boolean): void;
  (e: "confirm"): void;
}>();
</script>

<template>
  <Dialog :open="props.open" @update:open="(v) => emit('update:open', v)">
    <DialogContent class="sm:max-w-md">
      <DialogHeader>
        <DialogTitle class="flex items-center gap-2">
          <AlertTriangleIcon class="size-4 text-destructive" />
          {{ t("confirm.title") }}
        </DialogTitle>
        <DialogDescription class="text-xs">
          {{ t("confirm.description", props.paths.length) }}
        </DialogDescription>
      </DialogHeader>

      <div class="custom-scroll max-h-48 space-y-1 overflow-auto rounded-lg bg-muted/50 p-2">
        <p
          v-for="p in props.paths.slice(0, 5)"
          :key="p"
          class="num break-all font-mono text-[11px] text-muted-foreground"
        >
          {{ p }}
        </p>
        <p v-if="props.paths.length > 5" class="text-xs text-muted-foreground">
          {{ t("confirm.more", props.paths.length) }}
        </p>
      </div>

      <DialogFooter>
        <Button variant="outline" @click="emit('update:open', false)">
          {{ t("confirm.cancel") }}
        </Button>
        <Button variant="destructive" @click="emit('confirm')">
          {{ t("confirm.ok") }}
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
