<script setup lang="ts">
// 核心状态徽标：验证「壳 → 核心」链路已打通，并报告稿子落在哪。
//
// 纪律：这里不直接 invoke，也不碰文件系统——一律经 api 网关取数。
import { onMounted, ref } from "vue";
import { readDataHome, readEngineInfo, type DataHome, type EngineInfo } from "../api/core";

const info = ref<EngineInfo | null>(null);
const home = ref<DataHome | null>(null);
const error = ref<string | null>(null);

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
      <span class="badge__ver">
        engine {{ info.version }} · proto v{{ info.protocol_version }} · data v{{ info.data_format_version }}
      </span>
      <span v-if="home" class="badge__path" :title="`稿子在这里：${home.path}`">
        数据 {{ home.path }}
      </span>
    </template>
    <template v-else-if="error">核心未连接（浏览器预览模式）</template>
    <template v-else>连接核心中…</template>
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
