<script setup lang="ts">
/** 全局自定义右键菜单:替代 WebView 原生菜单。
 *  由 App.vue 在 contextmenu 事件里定位并打开,点击/Esc/失焦时关闭。 */
import { computed } from "vue";
import { HardDriveIcon, LanguagesIcon, MoonIcon, SparklesIcon, SunIcon, ZapIcon } from "@lucide/vue";
import { useRouter } from "vue-router";

import { currentLang, t, toggleLang } from "@/i18n";
import { dark, reduceMotion, toggleTheme, toggleMotion } from "@/composables/usePreferences";
import { fadeIn, fadeOut } from "@/lib/motion";

const props = defineProps<{
  state: { x: number; y: number } | null;
}>();

const emit = defineEmits<{
  (e: "close"): void;
}>();

const router = useRouter();

const MENU_W = 200;
const MENU_H = 250;

const style = computed(() => {
  if (!props.state) return {};
  return {
    left: `${Math.min(props.state.x, window.innerWidth - MENU_W - 8)}px`,
    top: `${Math.min(props.state.y, window.innerHeight - MENU_H - 8)}px`,
  };
});

function go(path: string): void {
  emit("close");
  void router.push(path);
}
</script>

<template>
  <Teleport to="body">
    <Transition :css="false" @enter="fadeIn" @leave="fadeOut">
      <div
        v-if="props.state"
        class="global-menu"
        :style="style"
        @contextmenu.prevent
        @click.stop
      >
        <div class="global-menu-label">{{ t("ctx.title") }}</div>
        <div class="global-menu-sep"></div>

        <button class="global-menu-item" @click="go('/')">
          <HardDriveIcon class="size-3.5" />
          {{ t("ctx.analyzer") }}
        </button>
        <button class="global-menu-item" @click="go('/cleaner')">
          <SparklesIcon class="size-3.5" />
          {{ t("ctx.cleaner") }}
        </button>

        <div class="global-menu-sep"></div>
        <div class="global-menu-label">{{ t("ctx.appearance") }}</div>

        <button class="global-menu-item" @click="emit('close'); toggleTheme()">
          <SunIcon v-if="dark" class="size-3.5" />
          <MoonIcon v-else class="size-3.5" />
          {{ t("ctx.theme") }}
        </button>
        <button class="global-menu-item" @click="emit('close'); toggleMotion()">
          <ZapIcon class="size-3.5" />
          {{ t("ctx.motion") }} · {{ reduceMotion ? "OFF" : "ON" }}
        </button>
        <button class="global-menu-item" @click="emit('close'); toggleLang()">
          <LanguagesIcon class="size-3.5" />
          {{ t("ctx.lang") }} · {{ currentLang === "zh" ? "EN" : "中文" }}
        </button>
      </div>
    </Transition>
  </Teleport>
</template>
