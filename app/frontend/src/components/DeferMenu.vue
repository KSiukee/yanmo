<script setup lang="ts">
// 延后菜单：**什么时候再问我**——五档预置 + 作者可以自己写一句。
//
// 五档的键与核心的预置档一一对应（`question.defer.*` 是话）；"写完这一章再问"那一档
// 要一个章节锚点，缺锚点时核心会明确报错（界面不替它兜默认）。
import { ref } from "vue";

import { t } from "../locales/index.ts";

const props = defineProps<{ busy: boolean }>();
const emit = defineEmits<{ confirm: [string, string]; cancel: [] }>();

const PRESETS = [
  "after_one_day",
  "after_three_days",
  "after_one_week",
  "when_chapter_written",
  "only_when_asked",
];

const preset = ref(PRESETS[0]);
const note = ref("");
</script>

<template>
  <section class="card card--form">
    <h3 class="sec">{{ t("flow.defer.title") }}</h3>
    <label v-for="key in PRESETS" :key="key" class="pick">
      <input v-model="preset" type="radio" :value="key" />
      <span>{{ t(`question.defer.${key}`) }}</span>
    </label>
    <input v-model="note" class="note" type="text" :placeholder="t('flow.defer.note_placeholder')" />
    <div class="row">
      <button class="act act--primary" :disabled="props.busy" @click="emit('confirm', preset, note)">
        {{ t("flow.defer.confirm") }}
      </button>
      <button class="act" :disabled="props.busy" @click="emit('cancel')">
        {{ t("flow.defer.cancel") }}
      </button>
    </div>
  </section>
</template>

<style scoped>
.card {
  padding: 8px;
  margin-bottom: 6px;
  border: 1px dashed var(--ym-line);
  border-radius: 6px;
  background: var(--ym-paper);
}
.sec {
  margin: 0 0 6px;
  font-size: 11px;
  opacity: 0.75;
}
.pick {
  display: flex;
  gap: 6px;
  align-items: center;
  padding: 2px 0;
  font-size: 12px;
}
.note {
  width: 100%;
  margin: 6px 0;
  padding: 4px 6px;
  font: inherit;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: var(--ym-paper);
  box-sizing: border-box;
}
.row {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.act {
  padding: 2px 8px;
  font-size: 11px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: transparent;
  cursor: pointer;
}
.act--primary {
  font-weight: 600;
}
.act:disabled {
  opacity: 0.5;
  cursor: default;
}
</style>
