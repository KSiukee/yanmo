<script setup lang="ts">
// 作答：把心里那一句写下来；想的话，顺手把它落进这一章的正文。
//
// 三条纪律（与核心一致）：
// 1. **不设任何「只能」**——键盘、口述、混着来都收（口述那条链路落地后接上，界面形状不变）；
//    文本与输入方式解耦：写的是文本，怎么打出来的另记一列（`source`）。
// 2. **落不落、落在哪由作者定**：落到光标处还是章末、要不要落，都不是默认替他做主的事——
//    落章会动正文，得他自己点那个勾。
// 3. **答完即定**：卡走到「已答」就不再回队列——所以保存前把这句话说在明处。
import { ref } from "vue";

import { t } from "../locales/index.ts";
import type { LandAt } from "./question.ts";

const props = defineProps<{
  busy: boolean;
  /** 落点：`cursor` 光标处 / `end` 章末（没在正文里点过时一律按章末，见会话层的说明） */
  landAt: LandAt;
  /** 记下之后要不要落进本章正文 */
  landToBody: boolean;
  /** 眼下有没有打开着的一章——没有的话落章那两个控件按不了（别放按不动的按钮） */
  canLand: boolean;
}>();
const emit = defineEmits<{
  save: [string];
  cancel: [];
  "update:landAt": [LandAt];
  "update:landToBody": [boolean];
}>();

const body = ref("");

function toggleLand(event: Event) {
  emit("update:landToBody", (event.target as HTMLInputElement).checked);
}
</script>

<template>
  <section class="card card--form">
    <h3 class="sec">{{ t("flow.answer.title") }}</h3>
    <textarea v-model="body" class="note" rows="4" :placeholder="t('flow.answer.placeholder')" />
    <p class="hint">{{ t("flow.answer.terminal") }}</p>

    <fieldset class="land" :disabled="props.busy || !props.canLand">
      <label class="land__label">
        <input type="checkbox" :checked="props.landToBody" @change="toggleLand" />
        {{ t("flow.land.toggle") }}
      </label>
      <span class="land__at">
        <label>
          <input
            type="radio"
            name="land-at"
            :checked="props.landAt === 'cursor'"
            @change="emit('update:landAt', 'cursor')"
          />
          {{ t("flow.land.cursor") }}
        </label>
        <label>
          <input
            type="radio"
            name="land-at"
            :checked="props.landAt === 'end'"
            @change="emit('update:landAt', 'end')"
          />
          {{ t("flow.land.end") }}
        </label>
      </span>
      <p class="hint">{{ props.canLand ? t("flow.land.cursor_hint") : t("flow.land.no_chapter") }}</p>
    </fieldset>

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
.land {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin: 0 0 6px;
  padding: 4px 6px;
  font-size: 11px;
  border: 1px dashed var(--ym-line);
  border-radius: 4px;
}
.land:disabled {
  opacity: 0.5;
}
.land__label,
.land__at label {
  display: inline-flex;
  gap: 4px;
  align-items: center;
}
.land__at {
  display: inline-flex;
  gap: 10px;
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
