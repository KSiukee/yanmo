<script setup lang="ts">
// 备份对话框里的「创作留痕包」一块：只放范围勾选、按钮与结果，逻辑在 composable。
//
// 单独成件的原因与设置页那几栏同一条：备份页已经贴着行数棘轮，而这一块的变化理由
// （留痕包长什么样）与备份（写哪儿、留几份）不是一回事。
import { t } from "../locales/index.ts";
import type { EditorSession } from "../editor/session";
import { useTraceExport } from "../composables/useTraceExport.ts";

const props = defineProps<{ session: EditorSession }>();
const { busy, note, workId, creativeOnly, run } = useTraceExport(props.session);
</script>

<template>
  <h3 class="trace-export__title">{{ t("backup.trace_button") }}</h3>
  <p class="trace-export__hint">{{ t("backup.trace_hint") }}</p>
  <label class="trace-export__hint">
    <input v-model="creativeOnly" type="checkbox" />
    {{ t("backup.trace_creative") }}
  </label>
  <div class="trace-export__actions">
    <button
      type="button"
      class="dialog__button"
      :disabled="busy || workId === null"
      @click="run"
    >
      {{ busy ? t("backup.running") : t("backup.trace_button") }}
    </button>
    <span v-if="note" class="trace-export__hint">{{ note }}</span>
  </div>
</template>

<style scoped src="./dialog.css"></style>

<style scoped>
.trace-export__title {
  margin: 14px 0 6px;
  font-size: 13px;
}

.trace-export__hint {
  display: block;
  margin: 0 0 6px;
  font-size: 12px;
  opacity: 0.8;
}

.trace-export__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
}
</style>
