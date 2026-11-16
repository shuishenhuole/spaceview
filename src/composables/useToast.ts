/** 全局操作反馈气泡:任意页面都能调用 setMessage,由 App.vue 壳统一渲染。 */
import { ref } from "vue";

export interface ToastMessage {
  id: number;
  text: string;
}

export const toast = ref<ToastMessage | null>(null);
let timer: number | undefined;
let seq = 0;

export function setMessage(text: string): void {
  const id = ++seq;
  toast.value = { id, text };
  if (timer !== undefined) window.clearTimeout(timer);
  timer = window.setTimeout(() => {
    if (toast.value?.id === id) toast.value = null;
  }, 5000);
}
