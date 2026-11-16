/** 页面路由。桌面端用 hash history,避免打包后自定义协议下的路径回退问题。 */
import { createRouter, createWebHashHistory } from "vue-router";

import AnalyzerView from "@/views/AnalyzerView.vue";
import CleanerView from "@/views/CleanerView.vue";

export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", name: "analyzer", component: AnalyzerView },
    { path: "/cleaner", name: "cleaner", component: CleanerView },
    { path: "/:pathMatch(.*)*", redirect: "/" },
  ],
});
