<script setup lang="ts">
// 作答：把心里那一句写下来。
//
// 三条纪律（与核心一致）：
// 1. **不设任何「只能」**——键盘、口述、混着来都收（口述那条链路落地后接上，界面形状不变）；
//    文本与输入方式解耦：写的是文本，怎么打出来的另记一列（`source`）。
// 2. **不动正文**：答案进答案池，这里一个字节都不写章里；
// 3. **答完即定**：卡走到「已答」就不再回队列——所以保存前把这句话说在明处，别让人以为还能改。
import { ref } from "vue";

import { t } from "../locales/index.ts";

const props = defineProps<{ busy: boolean }>();
const emit = defineEmits<{ save: [string]; cancel: [] }>();

const body = ref("");
</script>

<template>
  <section class="card card--form">
    <h3 class="sec">{{ t("flow.answer.title") }}</h3>
    <textarea v-model="body" class="note" rows="4" :placeholder="t('flow.answer.placeholder')" />
    <p class="hint">{{ t("flow.answer.terminal") }}</p>
    <div class="row">
      <button class="act act--primary" :disabled="props.busy || !body.trim()" @click="emit('save', body)">
        {{ t("flow.answer.save") }}
      </button>
      <button class="act" :disabled="props.busy" @click="emit('cancel')">
        {{ t("flow.answer.cancel") }}
      </button>
    </div>
  </section>
</template>

<style scoped>
.card {
  padding: 8px;
  margin-bottom: 6px;
  border: 1px solid var(--ym-line);
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
