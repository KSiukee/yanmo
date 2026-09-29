<script setup lang="ts">
// 「关于」那一栏：**只读报告**——版本一行 + 打开稿子文件夹 + 安全承诺与限制的摘要。
//
// 单独成组件：设置面板本身已经很长（上帝棘轮盯着 400 行），而这一栏自成一体、纯只读，
// 与别的设置项没有耦合。完整承诺清单在仓库的 SECURITY.md，这里只给摘要与入口。
import { computed, onMounted, ref } from "vue";

import { t } from "../locales/index.ts";
import { openDataDir, readAppVersion, readDataHome, readEngineInfo } from "../api/core";
import { readSlowReport, type SlowReport } from "../api/perf";
import { secondsText } from "../editor/drift";
import { stallStats } from "../editor/stall-watch";
import type { EditorSession } from "../editor/session";

const props = defineProps<{ session: EditorSession }>();
/** 稿子路径：**只在「稿子放在哪」那一组显示一次**，这里只拿它决定按钮能不能点 */
const dataPath = computed(() => props.session.location.info.value?.path ?? "");

const appVersion = ref("");
const engineVersion = ref("");
const schemaVersion = ref<number | null>(null);
const aboutError = ref<string | null>(null);
/** 打开稿子文件夹失败的原因（点了没反应最难查，所以要把原因说出来） */
const openError = ref<string | null>(null);

// 运行诊断：**界面卡多久**（前端当场量到的，模块级单例）与**哪条命令跑得久**
//（壳里那本账）。前者是现成的读数，后者打开这一栏时读一次——它是现场，不是要盯着的仪表。
const slow = ref<SlowReport | null>(null);
const diagError = ref<string | null>(null);

/** 界面那一行：没卡过就说没卡过（**空态也要说清楚口径**，免得看着像读数坏了）。 */
const uiLine = computed(() =>
  stallStats.value.count === 0
    ? t("settings.diag_ui_none")
    : t("settings.diag_ui_some", {
        count: stallStats.value.count,
        max: secondsText(stallStats.value.maxMs),
        total: secondsText(stallStats.value.totalMs),
      }),
);

/** 命令那一行：只报条数，明细在下面那张小表里。 */
const commandLine = computed(() => {
  const recent = slow.value?.recent ?? [];
  return recent.length === 0
    ? t("settings.diag_cmd_none")
    : t("settings.diag_cmd_some", { count: recent.length });
});

/** 明细（最多五条：这一栏是给人扫一眼的，不是日志查看器）。 */
const commandItems = computed(() =>
  (slow.value?.recent ?? [])
    .slice(0, 5)
    .map((entry) =>
      t("settings.diag_cmd_item", {
        command: entry.command,
        seconds: secondsText(entry.ms),
      }),
    ),
);

/** **此刻还在飞**的那条——"界面好像没反应"的时候，这一行就是答案。 */
const inflightLine = computed(() => {
  const flying = slow.value?.inflight?.[0];
  return flying
    ? t("settings.diag_cmd_inflight", {
        command: flying.command,
        seconds: secondsText(flying.ms),
      })
    : "";
});

/** 库里记着的那一条（界面被重载过之后，壳里那份内存记录就没了，只有它还在）。 */
const lastInDbLine = computed(() => {
  const last = slow.value?.last_in_db;
  return last
    ? t("settings.diag_cmd_last", {
        command: last.command,
        seconds: secondsText(last.ms),
      })
    : "";
});
/** 一行报全：安装包版本 / 引擎版本 / 库结构版本 */
const aboutVersion = computed(() =>
  t("settings.about_version_value", {
    version: appVersion.value || "—",
    engine: engineVersion.value || "—",
    schema: schemaVersion.value ?? "—",
  }),
);

async function openDataFolder(): Promise<void> {
  try {
    await openDataDir();
    openError.value = null;
  } catch (error) {
    openError.value = error instanceof Error ? error.message : String(error);
  }
}

onMounted(async () => {
  // 版本号单独兜底：读不到不该把整块"关于"打成错误
  appVersion.value = await readAppVersion().catch(() => "");
  try {
    const [engine, home] = await Promise.all([readEngineInfo(), readDataHome()]);
    engineVersion.value = engine.version;
    schemaVersion.value = home.schema_version;
  } catch (error) {
    aboutError.value = error instanceof Error ? error.message : String(error);
  }
  // 慢命令读数：读不到不影响这一栏别的（版本、承诺那几句照旧显示）
  slow.value = await readSlowReport().catch((error: unknown) => {
    diagError.value = error instanceof Error ? error.message : String(error);
    return null;
  });
});
</script>
<template>
  <p class="settings__group">{{ t("settings.group_about") }}</p>
  <p class="settings__row">
    <span class="settings__label">{{ t("settings.about_version") }}</span>
    <code class="settings__path">{{ aboutVersion }}</code>
  </p>
  <!-- 路径**只在「稿子放在哪」那一组显示一次**：这里留着按钮就够了（它的标签已经说清是打开哪个文件夹）。
       两处都摊一遍绝对路径，看着像出了错，也让人不知道该信哪一处。 -->
  <button
    type="button"
    class="settings__button dialog__button"
    :disabled="!dataPath"
    @click="void openDataFolder()"
  >
    {{ t("settings.about_open_data") }}
  </button>
  <span v-if="openError" class="settings__hint settings__hint--bad">{{ openError }}</span>
  <p class="settings__hint">{{ t("settings.about_promise") }}</p>
  <p class="settings__hint">{{ t("settings.about_limits") }}</p>
  <p v-if="aboutError" class="settings__hint settings__hint--bad">{{ aboutError }}</p>

  <p class="settings__group">{{ t("settings.group_diagnostics") }}</p>
  <p class="settings__hint">{{ uiLine }}</p>
  <p class="settings__hint">{{ commandLine }}</p>
  <p v-if="inflightLine" class="settings__hint">{{ inflightLine }}</p>
  <!-- 明细最多五条（新的在前）：这一栏是给人扫一眼的，不是日志查看器 -->
  <ul v-if="commandItems.length" class="settings__plan">
    <li v-for="item in commandItems" :key="item" class="settings__plan-row">{{ item }}</li>
  </ul>
  <p v-if="lastInDbLine" class="settings__hint">{{ lastInDbLine }}</p>
  <p class="settings__hint">{{ t("settings.diag_note") }}</p>
  <p v-if="diagError" class="settings__hint settings__hint--bad">
    {{ t("settings.diag_error", { detail: diagError }) }}
  </p>
</template>

<style scoped src="./settings-dialog.css"></style>
