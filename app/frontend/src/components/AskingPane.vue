<script setup lang="ts">
// 正在问的那一张：句子 + 处置六件事（说好 / 作答 / 延后 / 舍弃 / 静音这一类 / 记灵感）。
//
// 单独成件的原因：它与"候选怎么排、答完落哪儿"的变化理由不一样——
// 那边是机制与流程，这边只是"这一屏上摆哪几个按钮"。它**纯展示**：动作交回面板那层去调命令。
import { t } from "../locales/index.ts";
import { sourceLabel } from "./question.ts";

const props = defineProps<{
  card: { body: string; source: string };
  busy: boolean;
  /** 模式 A 才有「跳过」：跳过不处置、不落章，直接看下一张 */
  canSkip: boolean;
}>();

const emit = defineEmits<{
  praise: [];
  answer: [];
  defer: [];
  discard: [];
  muteClass: [];
  /** 只对模块来的问题显示：不再听这个来源（核心不受牵连） */
  muteSource: [string];
  inspire: [];
  skip: [];
}>();
</script>

<template>
  <section class="asking">
    <h3 class="sec">{{ t("flow.asking") }}</h3>
    <p class="src-row">
      <span class="src" :class="{ 'src--module': props.card.source !== 'core' }">
        {{ sourceLabel(props.card.source) }}
      </span>
    </p>
    <p class="body">{{ props.card.body }}</p>
    <div class="row">
      <button class="act" :disabled="props.busy" @click="emit('praise')">
        {{ t("flow.action.praise") }}
      </button>
      <button class="act act--answer" :disabled="props.busy" @click="emit('answer')">
        {{ t("flow.action.answer") }}
      </button>
      <button class="act" :disabled="props.busy" @click="emit('defer')">
        {{ t("flow.action.defer") }}
      </button>
      <button class="act" :disabled="props.busy" @click="emit('discard')">
        {{ t("flow.action.discard") }}
      </button>
      <button class="act" :disabled="props.busy" @click="emit('muteClass')">
        {{ t("flow.action.mute_class") }}
      </button>
      <button
        v-if="props.card.source !== 'core'"
        class="act"
        :disabled="props.busy"
        :title="t('flow.source.mute_title', { name: sourceLabel(props.card.source) })"
        @click="emit('muteSource', props.card.source)"
      >
        {{ t("flow.source.mute") }}
      </button>
      <button class="act" :disabled="props.busy" @click="emit('inspire')">
        {{ t("flow.action.inspire") }}
      </button>
      <button v-if="props.canSkip" class="act" :disabled="props.busy" @click="emit('skip')">
        {{ t("flow.action.skip") }}
      </button>
    </div>
  </section>
</template>

<style scoped>
.sec {
  margin: 12px 0 6px;
  font-size: 11px;
  opacity: 0.75;
}
.body {
  margin: 0 0 8px;
  line-height: 1.6;
}
/* 来源：核心与模块**一眼可辨**（模块用强调色；核心保持安静） */
.src-row {
  margin: 0 0 4px;
  font-size: 11px;
  opacity: 0.8;
}
.src {
  padding: 0 5px;
  border: 1px solid var(--ym-line);
  border-radius: 8px;
}
.src--module {
  border-color: var(--ym-accent);
  color: var(--ym-accent);
}
.row {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
  font-size: 11px;
  opacity: 0.85;
}
.act {
  padding: 2px 8px;
  font-size: 11px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: transparent;
  cursor: pointer;
}
.act:disabled {
  opacity: 0.5;
  cursor: default;
}
/* 作答是这一屏上唯一的"写点什么"入口，给它一点分量 */
.act--answer {
  font-weight: 600;
}
</style>
