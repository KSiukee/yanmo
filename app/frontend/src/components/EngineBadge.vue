<script setup lang="ts">
// 核心状态徽标：验证「壳 → 核心」链路已打通，并报告稿子落在哪。
//
// 纪律：这里不直接 invoke，也不碰文件系统——一律经 api 网关取数。
import { computed, onMounted, ref } from "vue";

import { readDataHome, readEngineInfo, type DataHome, type EngineInfo } from "../api/core";
import { t } from "../locales/index.ts";

const info = ref<EngineInfo | null>(null);
const home = ref<DataHome | null>(null);
const error = ref<string | null>(null);

const engineLine = computed(() =>
  t("badge.engine", {
    version: info.value?.version ?? "",
    protocol: info.value?.protocol_version ?? "",
    format: info.value?.data_format_version ?? "",
  }),
);

onMounted(async () => {
  try {
    [info.value, home.value] = await Promise.all([readEngineInfo(), readDataHome()]);
  } catch (e) {
    // 浏览器里直接跑时没有 Tauri 运行时，属预期情况。
    error.value = e instanceof Error ? e.message : String(e);
  }
});
</script>

<template>
  <span class="badge">
    <template v-if="info">
      <span class="badge__ver">{{ engineLine }}</span>
      <span v-if="home" class="badge__path" :title="t('badge.data_path_title', { path: home.path })">
        {{ t("badge.data_path", { path: home.path }) }}
      </span>
    </template>
    <template v-else-if="error">{{ t("badge.no_core") }}</template>
    <template v-else>{{ t("badge.connecting") }}</template>
  </span>
</template>

<style scoped>
.badge {
  margin-left: auto;
  display: flex;
  align-items: baseline;
  gap: 10px;
  min-width: 0;
  font-size: 11px;
  font-variant-numeric: tabular-nums;
  color: var(--ym-ink-soft);
}

.badge__path {
  max-width: 40ch;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  opacity: 0.85;
}
</style>
