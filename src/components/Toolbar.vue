<script setup lang="ts">
/** 顶部工具栏:品牌标识、扫描目标选择、开始/取消/暂停、主题/动效/语言切换。
 *  只负责渲染并上报动作,所有状态变更统一在 App.vue 执行。 */
import { computed } from "vue";
import {
  ActivityIcon,
  FolderOpenIcon,
  LanguagesIcon,
  LoaderCircleIcon,
  MoonIcon,
  PauseIcon,
  PlayIcon,
  SunIcon,
  XIcon,
} from "@lucide/vue";

import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { currentLang, t } from "@/i18n";
import logoUrl from "@/assets/logo.png";
import type { AppPhase } from "@/types";

const props = defineProps<{
  drives: string[];
  driveValue: string;
  customPath: string | null;
  targetPath: string;
  phase: AppPhase;
  paused: boolean;
  deleting: boolean;
  dark: boolean;
  reduceMotion: boolean;
}>();

const emit = defineEmits<{
  (e: "select-drive", value: string): void;
  (e: "browse"): void;
  (e: "start"): void;
  (e: "cancel"): void;
  (e: "toggle-pause"): void;
  (e: "toggle-theme"): void;
  (e: "toggle-motion"): void;
  (e: "toggle-lang"): void;
}>();

/** 扫描或后台删除期间禁止修改目标 */
const busy = computed(() => props.phase === "scanning" || props.deleting);
</script>

<template>
  <header class="panel app-toolbar mx-3 mt-3 flex shrink-0 items-center gap-2 px-3 py-2">
    <div class="brand-logo">
      <img :src="logoUrl" alt="SpaceView" class="size-full" />
    </div>
    <div class="mr-1 hidden flex-col leading-tight sm:flex">
      <span class="text-sm font-semibold tracking-tight">SpaceView</span>
      <span class="text-[10px] text-muted-foreground">Disk Space Analyzer</span>
    </div>

    <Separator orientation="vertical" class="mx-1 h-6" />

    <Select
      :model-value="props.driveValue"
      :disabled="busy"
      @update:model-value="(v) => emit('select-drive', String(v))"
    >
      <SelectTrigger size="sm" class="h-8 w-28 text-xs">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        <SelectItem v-for="(d, i) in props.drives" :key="d" :value="String(i)">
          {{ d }}
        </SelectItem>
      </SelectContent>
    </Select>

    <span
      v-if="props.customPath"
      class="rounded-md bg-brand-soft px-2 py-1 text-[10px] font-medium text-brand"
    >
      {{ t("toolbar.custom") }}
    </span>

    <Button variant="outline" size="sm" :disabled="busy" @click="emit('browse')">
      <FolderOpenIcon />
      {{ t("toolbar.browse") }}
    </Button>

    <span class="num hidden min-w-0 truncate text-xs text-muted-foreground lg:inline">
      {{ t("toolbar.target", props.targetPath) }}
    </span>

    <div class="ml-auto flex shrink-0 items-center gap-1.5">
      <Tooltip>
        <TooltipTrigger as-child>
          <Button variant="ghost" size="icon-sm" @click="emit('toggle-lang')">
            <LanguagesIcon />
          </Button>
        </TooltipTrigger>
        <TooltipContent>
          {{ currentLang === "zh" ? t("toolbar.langZh") : t("toolbar.langEn") }}
        </TooltipContent>
      </Tooltip>

      <Tooltip>
        <TooltipTrigger as-child>
          <Button variant="ghost" size="icon-sm" @click="emit('toggle-theme')">
            <SunIcon v-if="props.dark" />
            <MoonIcon v-else />
          </Button>
        </TooltipTrigger>
        <TooltipContent>
          {{ props.dark ? t("toolbar.light") : t("toolbar.dark") }}
        </TooltipContent>
      </Tooltip>

      <Tooltip>
        <TooltipTrigger as-child>
          <Button
            variant="ghost"
            size="icon-sm"
            :class="props.reduceMotion ? 'text-muted-foreground' : 'text-brand'"
            @click="emit('toggle-motion')"
          >
            <ActivityIcon />
          </Button>
        </TooltipTrigger>
        <TooltipContent>
          {{ props.reduceMotion ? t("toolbar.motionOff") : t("toolbar.motionOn") }}
        </TooltipContent>
      </Tooltip>

      <Separator orientation="vertical" class="mx-1 h-6" />

      <template v-if="props.phase === 'scanning'">
        <Button variant="outline" size="sm" @click="emit('toggle-pause')">
          <PauseIcon v-if="!props.paused" />
          <PlayIcon v-else />
          {{ props.paused ? t("toolbar.resume") : t("toolbar.pause") }}
        </Button>
        <Button variant="destructive" size="sm" @click="emit('cancel')">
          <XIcon />
          {{ t("toolbar.cancel") }}
        </Button>
        <LoaderCircleIcon v-if="!props.paused" class="size-4 animate-spin text-brand" />
      </template>

      <template v-else-if="props.deleting">
        <span class="text-xs text-muted-foreground">{{ t("toolbar.deleting") }}</span>
        <LoaderCircleIcon class="size-4 animate-spin text-brand" />
      </template>

      <Button v-else @click="emit('start')">
        <PlayIcon />
        {{ t("toolbar.start") }}
      </Button>
    </div>
  </header>
</template>
