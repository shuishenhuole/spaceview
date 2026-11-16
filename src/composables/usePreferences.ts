/** 全局偏好(模块级共享):深色主题 / 减少动画。
 *  实际应用到 DOM 与原生标题栏由 App.vue 壳统一执行,各视图只读这两个 ref。 */
import { ref } from "vue";

export const dark = ref(localStorage.getItem("spaceview.theme") !== "light");
export const reduceMotion = ref(
  localStorage.getItem("spaceview.motion") === "reduced",
);

export function toggleTheme(): void {
  dark.value = !dark.value;
}

export function toggleMotion(): void {
  reduceMotion.value = !reduceMotion.value;
}
