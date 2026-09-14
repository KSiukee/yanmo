<script setup lang="ts">
// 「关于」那一栏：**只读报告**——版本一行 + 打开稿子文件夹 + 安全承诺与限制的摘要。
//
// 单独成组件：设置面板本身已经很长（上帝棘轮盯着 400 行），而这一栏自成一体、纯只读，
// 与别的设置项没有耦合。完整承诺清单在仓库的 SECURITY.md，这里只给摘要与入口。
import { computed, onMounted, ref } from "vue";

import { t } from "../locales/index.ts";
import { openDataDir, readAppVersion, readDataHome, readEngineInfo } from "../api/core";
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
</template>

<style scoped src="./settings-dialog.css"></style>
