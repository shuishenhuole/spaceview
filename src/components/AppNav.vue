<script setup lang="ts">
/** 两个页面共用的顶部导航(hash 路由,桌面端无需服务端回退) */
import { HardDriveIcon, SparklesIcon } from "@lucide/vue";
import { RouterLink } from "vue-router";

import { t } from "@/i18n";
</script>

<template>
  <nav class="flex shrink-0 items-center gap-1">
    <RouterLink to="/" class="nav-link" exact-active-class="nav-link-active">
      <HardDriveIcon class="size-3.5" />
      {{ t("nav.analyzer") }}
    </RouterLink>
    <RouterLink to="/cleaner" class="nav-link" active-class="nav-link-active">
      <SparklesIcon class="size-3.5" />
      {{ t("nav.cleaner") }}
    </RouterLink>
  </nav>
</template>
