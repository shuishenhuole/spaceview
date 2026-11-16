<script setup lang="ts">
/** 应用壳:全局偏好(应用 DOM 与原生标题栏)、路由视图、
 *  全局自定义右键菜单与操作反馈 Toast。 */
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import { RouterView } from "vue-router";

import { api } from "@/api";
import GlobalContextMenu from "@/components/GlobalContextMenu.vue";
import { TooltipProvider } from "@/components/ui/tooltip";
import Toast from "@/components/Toast.vue";
import { currentLang } from "@/i18n";
import { dark, reduceMotion } from "@/composables/usePreferences";
import { toast } from "@/composables/useToast";
import { flashTheme, pageEnter, pageLeave } from "@/lib/motion";

/** 全局自定义右键菜单的位置;null 表示关闭 */
const menu = ref<{ x: number; y: number } | null>(null);

watch(
  dark,
  (v) => {
    document.documentElement.classList.toggle("dark", v);
    localStorage.setItem("spaceview.theme", v ? "dark" : "light");
    // 同步 Windows 原生标题栏(DWM 沉浸深色)与 WebView2 的 prefers-color-scheme
    void api.setTitlebarTheme(v).catch((e) =>
      console.warn("[SpaceView] set_titlebar_theme:", e),
    );
    // 切换瞬间给界面一个极轻的亮度反馈(减少动画时自动跳过)
    flashTheme(document.documentElement);
  },
  { immediate: true },
);

watch(reduceMotion, (v) => {
  document.documentElement.classList.toggle("reduce-motion", v);
  localStorage.setItem("spaceview.motion", v ? "reduced" : "full");
});

watch(currentLang, (v) => {
  document.documentElement.lang = v === "zh" ? "zh-CN" : "en";
});

/** 禁用 WebView 原生右键,统一替换为自定义菜单;
 *  文件行 / Treemap 块等已有各自的菜单(reka-ui ContextMenuTrigger),跳过。 */
function onContextMenu(e: MouseEvent): void {
  const target = e.target as HTMLElement | null;
  const handledByRow = !!target?.closest('[data-slot="context-menu-trigger"]');
  e.preventDefault();
  if (handledByRow) {
    menu.value = null;
    return;
  }
  menu.value = { x: e.clientX, y: e.clientY };
}

function closeMenu(): void {
  menu.value = null;
}

function onKeydown(e: KeyboardEvent): void {
  if (e.key === "Escape") closeMenu();
}

onMounted(() => {
  // 窗口完全就绪后再下发一次,确保 DWM 属性在标题栏可见后生效
  void api.setTitlebarTheme(dark.value).catch(() => {});
  window.addEventListener("click", closeMenu);
  window.addEventListener("blur", closeMenu);
  window.addEventListener("resize", closeMenu);
  window.addEventListener("keydown", onKeydown);
});

onBeforeUnmount(() => {
  window.removeEventListener("click", closeMenu);
  window.removeEventListener("blur", closeMenu);
  window.removeEventListener("resize", closeMenu);
  window.removeEventListener("keydown", onKeydown);
});
</script>

<template>
  <TooltipProvider :delay-duration="350">
    <div
      class="app-shell"
      :class="{ 'reduce-motion': reduceMotion }"
      @contextmenu="onContextMenu"
    >
      <!-- 页面切换统一动效:淡出旧页 → 淡入上移新页(减少动画偏好时自动跳过) -->
      <RouterView v-slot="{ Component }">
        <Transition mode="out-in" :css="false" @enter="pageEnter" @leave="pageLeave">
          <component :is="Component" />
        </Transition>
      </RouterView>
      <Toast :message="toast?.text ?? null" />
      <GlobalContextMenu :state="menu" @close="closeMenu" />
    </div>
  </TooltipProvider>
</template>
