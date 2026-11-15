<script setup lang="ts">
import { CopyIcon, FolderOpenIcon, Trash2Icon } from "@lucide/vue";

import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { t } from "@/i18n";
import type { RowAction, RowActionType } from "@/types";

const props = defineProps<{
  path: string;
  isDir: boolean;
  disabled?: boolean;
}>();

const emit = defineEmits<{
  (e: "action", action: RowAction): void;
}>();

function send(type: RowActionType): void {
  emit("action", { type, path: props.path, isDir: props.isDir });
}
</script>

<template>
  <ContextMenu>
    <ContextMenuTrigger as-child>
      <slot />
    </ContextMenuTrigger>
    <ContextMenuContent v-if="!disabled" class="w-56">
      <ContextMenuItem @select="send('copy')">
        <CopyIcon />
        {{ t("menu.copy") }}
      </ContextMenuItem>
      <ContextMenuItem @select="send('reveal')">
        <FolderOpenIcon />
        {{ t("menu.reveal") }}
      </ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem variant="destructive" @select="send('delete')">
        <Trash2Icon />
        {{ t("menu.delete") }}
      </ContextMenuItem>
    </ContextMenuContent>
  </ContextMenu>
</template>
