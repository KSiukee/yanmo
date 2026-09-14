<script setup lang="ts">
// 核心状态徽标：报告"这是哪一版 + 稿子落在哪"。
//
// 界面上**只留作者用得上的**：安装包版本、库结构版本（能不能打开这个库看它）、稿子路径。
// 引擎 / 协议 / 数据格式这些内部号收进悬停提示——它们在混搭与排查时才有用，
// 天天摆在眼前只是噪音（`proto v1` 尤其：那个协议现在还没实现）。
//
// 纪律：这里不直接 invoke，也不碰文件系统——一律经 api 网关取数。
import { computed, onMounted, ref } from "vue";

import {
  openDataDir,
  readAppVersion,
  readDataHome,
  readEngineInfo,
  type DataHome,
  type EngineInfo,
} from "../api/core";
import { t } from "../locales/index.ts";

const info = ref<EngineInfo | null>(null);
const home = ref<DataHome | null>(null);
/** 安装包版本（读不到就退回引擎版本——两者今天同号，差不到哪去） */
const appVersion = ref<string | null>(null);
const error = ref<string | null>(null);
/** 打开文件夹失败的原因（极少发生；但失败也要说出来，别点了没反应） */
const openError = ref<string | null>(null);

const versionLine = computed(() =>
  t("badge.version", {
    version: appVersion.value ?? info.value?.version ?? "",
    schema: home.value?.schema_version ?? "",
  }),
);

/** 悬停才看的内部号：引擎 / 协议 / 数据格式（排查与混搭时用） */
const versionTitle = computed(() =>
  t("badge.version_title", {
    engine: info.value?.version ?? "",
    protocol: info.value?.protocol_version ?? "",
    format: info.value?.data_format_version ?? "",
  }),
);

/** 点一下「数据 …」：让系统打开稿子所在的文件夹（壳自己打开自己的目录，界面不传路径）。 */
async function openFolder(): Promise<void> {
  try {
    await openDataDir();
    openError.value = null;
  } catch (e) {
    openError.value = e instanceof Error ? e.message : String(e);
  }
}

onMounted(async () => {
  try {
    [info.value, home.value] = await Promise.all([readEngineInfo(), readDataHome()]);
    // 版本号单独读、单独兜底：读不到不该把整个徽标打回"未连接"
    appVersion.value = await readAppVersion().catch(() => null);
  } catch (e) {
    // 浏览器里直接跑时没有 Tauri 运行时，属预期情况。
    error.value = e instanceof Error ? e.message : String(e);
  }
});
</script>

<template>
  <span class="badge">
    <template v-if="info">
      <span class="badge__ver" :title="versionTitle">{{ versionLine }}</span>
      <button
        v-if="home"
        type="button"
        class="badge__path"
        :title="t('badge.data_path_title', { path: home.path })"
        @click="openFolder()"
      >
        {{ t("badge.data_path", { path: home.path }) }}
      </button>
      <span v-if="openError" class="badge__bad">{{ openError }}</span>
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
  padding: 0;
  border: none;
  background: none;
  color: inherit;
  font: inherit;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  opacity: 0.85;
  cursor: pointer;
}

.badge__path:hover {
  color: var(--ym-accent);
  text-decoration: underline;
}

.badge__bad {
  color: #b3261e;
}
</style>
