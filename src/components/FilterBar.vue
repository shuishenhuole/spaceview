<script setup lang="ts">
import { ListFilterIcon, SearchIcon, XIcon } from "@lucide/vue";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { t } from "@/i18n";

const props = defineProps<{
  filterText: string;
  minBytes: number;
}>();

const emit = defineEmits<{
  (e: "update:filterText", value: string): void;
  (e: "update:minBytes", value: number): void;
}>();
</script>

<template>
  <div class="panel flex items-center gap-2 px-3 py-2">
    <ListFilterIcon class="size-3.5 shrink-0 text-brand" />
    <span class="shrink-0 text-xs font-medium text-muted-foreground">
      {{ t("filter.label") }}
    </span>

    <div class="relative w-64 shrink-0">
      <SearchIcon
        class="absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground"
      />
      <Input
        :model-value="props.filterText"
        class="h-8 pl-7 text-xs"
        :placeholder="t('filter.placeholder')"
        @update:model-value="(v) => emit('update:filterText', String(v))"
      />
      <Button
        v-if="props.filterText"
        variant="ghost"
        size="icon-xs"
        class="absolute top-1/2 right-1 -translate-y-1/2"
        @click="emit('update:filterText', '')"
      >
        <XIcon />
      </Button>
    </div>

    <Select
      :model-value="String(props.minBytes)"
      @update:model-value="(v) => emit('update:minBytes', Number(v))"
    >
      <SelectTrigger size="sm" class="h-8 w-32 text-xs">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="0">{{ t("filter.any") }}</SelectItem>
        <SelectItem value="10485760">{{ t("filter.m10") }}</SelectItem>
        <SelectItem value="104857600">{{ t("filter.m100") }}</SelectItem>
        <SelectItem value="1073741824">{{ t("filter.g1") }}</SelectItem>
      </SelectContent>
    </Select>
  </div>
</template>
