<script setup lang="ts">
// 记灵感：**一句话就够，不打断手上这件事**。
//
// 纪律（与核心一致）：记完回到原问题、问题的状态一个字节都不变——所以这里只有
// 一个输入框与两个按钮，没有"顺便把它延后"之类会改状态的勾选框。
// 口述那条路（语音引擎）落地后，`source` 换成 voice / mixed，界面形状不变。
import { ref } from "vue";

import { t } from "../locales/index.ts";

const props = defineProps<{ busy: boolean }>();
const emit = defineEmits<{ save: [string]; cancel: [] }>();

const body = ref("");
</script>

<template>
  <section class="card card--form">
    <h3 class="sec">{{ t("flow.inspire.title") }}</h3>
    <textarea v-model="body" class="note" rows="3" :placeholder="t('flow.inspire.placeholder')" />
    <p class="hint">{{ t("flow.inspire.orthogonal") }}</p>
    <div class="row">
      <button class="act act--primary" :disabled="props.busy || !body.trim()" @click="emit('save', body)">
        {{ t("flow.inspire.save") }}
      </button>
      <button class="act" :disabled="props.busy" @click="emit('cancel')">
        {{ t("flow.inspire.cancel") }}
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
.hint {
  margin: 0 0 6px;
  font-size: 11px;
  line-height: 1.5;
  opacity: 0.7;
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
