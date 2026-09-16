<script setup lang="ts">
// 作答：把心里那一句写下来；想的话，顺手把它送到该去的地方。
//
// 三条纪律（与核心一致）：
// 1. **不设任何「只能」**——键盘、口述、混着来都收（口述那条链路落地后接上，界面形状不变）；
//    文本与输入方式解耦：写的是文本，怎么打出来的另记一列（`source`）。
// 2. **落不落、落到哪儿由作者定**：正文段落 / 章纲（这一章的一句话）/ 场景卡（新建一张），
//    三个都摆在明处；落地会动稿子或动结构，得他自己点那个勾。
// 3. **答完即定**：卡走到「已答」就不再回队列——所以保存前把这句话说在明处。
import { ref } from "vue";

import { t } from "../locales/index.ts";
import type { AnswerTarget } from "../api/question.ts";
import { ANSWER_TARGETS, landLabel, targetLabel, type LandAt } from "./question.ts";

const props = defineProps<{
  busy: boolean;
  /** 落到哪儿：正文段落 / 章纲 / 场景卡 */
  target: AnswerTarget;
  /** 场景卡的名字（只对"场景卡"有意义；空着就留给作者在树上起名） */
  sceneTitle: string;
  /** 落到正文时的落点：`cursor` 光标处 / `end` 章末 */
  landAt: LandAt;
  /** 记下之后要不要落到这一章（不勾就只是把答案记进答案池） */
  landOnAnswer: boolean;
  /** 眼下有没有打开着的一章——没有的话落地那些控件按不了（别放按不动的按钮） */
  canLand: boolean;
  /** 现在走的是「先问后排版」：这一轮的答案先攒着，一轮问完才落——所以这里不问落点 */
  collecting: boolean;
}>();

const emit = defineEmits<{
  save: [string];
  cancel: [];
  "update:target": [AnswerTarget];
  "update:sceneTitle": [string];
  "update:landAt": [LandAt];
  "update:landOnAnswer": [boolean];
}>();

const body = ref("");

function toggleLand(event: Event) {
  emit("update:landOnAnswer", (event.target as HTMLInputElement).checked);
}
function setTitle(event: Event) {
  emit("update:sceneTitle", (event.target as HTMLInputElement).value);
}
</script>

<template>
  <section class="card card--form">
    <h3 class="sec">{{ t("flow.answer.title") }}</h3>
    <textarea v-model="body" class="note" rows="4" :placeholder="t('flow.answer.placeholder')" />
    <p class="hint">{{ t("flow.answer.terminal") }}</p>

    <!-- 先问后排版：这一条先攒着，落到哪儿要到"一轮问够了"那一步才定 -->
    <p v-if="props.collecting" class="hint">{{ t("flow.round.collect_hint") }}</p>
    <fieldset v-else class="land" :disabled="props.busy || !props.canLand">
      <label class="land__label">
        <input type="checkbox" :checked="props.landOnAnswer" @change="toggleLand" />
        {{ t("flow.land.toggle") }}
      </label>
      <span class="land__at">
        <label v-for="option in ANSWER_TARGETS" :key="option">
          <input
            type="radio"
            name="land-target"
            :checked="props.target === option"
            @change="emit('update:target', option)"
          />
          {{ targetLabel(option) }}
        </label>
      </span>
      <!-- 落到正文才问"落在哪儿" -->
      <span v-if="props.target === 'body'" class="land__at">
        <label>
          <input
            type="radio"
            name="land-at"
            :checked="props.landAt === 'cursor'"
            @change="emit('update:landAt', 'cursor')"
          />
          {{ landLabel("cursor") }}
        </label>
        <label>
          <input
            type="radio"
            name="land-at"
            :checked="props.landAt === 'end'"
            @change="emit('update:landAt', 'end')"
          />
          {{ landLabel("end") }}
        </label>
      </span>
      <!-- 场景卡要个名字（空着也行，回头在树上改） -->
      <input
        v-if="props.target === 'scene'"
        class="land__title"
        type="text"
        :value="props.sceneTitle"
        :placeholder="t('flow.target.scene_title')"
        @change="setTitle"
      />
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
  flex-wrap: wrap;
  gap: 10px;
}
.land__title {
  padding: 2px 6px;
  font: inherit;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: var(--ym-paper);
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
