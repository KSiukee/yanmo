<script setup lang="ts">
// 一条候选问题：句子 + 「为什么先问它」+ 三个动作。
//
// 句子是**核心给的文本**（下拉卡时就由界面按字典渲染好存下来了），这里只负责摆；
// 引力拆解是机制算出来的（`reasonLines`），点开才显示——作者想看原因时才看，不占版面。
import { ref } from "vue";

import { t } from "../locales/index.ts";
import type { SelectedQuestion } from "../api/question.ts";
import { reasonLines, sourceLabel } from "./question.ts";

const props = defineProps<{ item: SelectedQuestion; busy: boolean }>();
const emit = defineEmits<{
  ask: [SelectedQuestion];
  inspire: [number];
  /** 只对模块来的问题显示：不再听这个来源（核心不受牵连） */
  muteSource: [string];
}>();

const whyOpen = ref(false);
</script>

<template>
  <article class="card">
    <p class="card__src">
      <span class="src" :class="{ 'src--module': props.item.source !== 'core' }">
        {{ sourceLabel(props.item.source) }}
      </span>
    </p>
    <p class="card__body" @click="emit('ask', props.item)">{{ props.item.body }}</p>
    <div class="row">
      <button class="link" @click="whyOpen = !whyOpen">{{ t("flow.why") }}</button>
      <button class="link" :disabled="props.busy" @click="emit('ask', props.item)">
        {{ t("flow.action.ask") }}
      </button>
      <button class="link" :disabled="props.busy" @click="emit('inspire', props.item.card_id)">
        {{ t("flow.action.inspire") }}
      </button>
      <button
        v-if="props.item.source !== 'core'"
        class="link"
        :disabled="props.busy"
        :title="t('flow.source.mute_title', { name: sourceLabel(props.item.source) })"
        @click="emit('muteSource', props.item.source)"
      >
        {{ t("flow.source.mute") }}
      </button>
    </div>
    <ul v-if="whyOpen" class="why">
      <li v-for="line in reasonLines(props.item.gravity)" :key="line.label">
        {{ line.label }}：{{ line.value }}
      </li>
    </ul>
  </article>
</template>

<style scoped>
.card {
  padding: 8px;
  margin-bottom: 6px;
  border: 1px solid var(--ym-line);
  border-radius: 6px;
  background: var(--ym-paper);
}
/* 来源：核心与模块**一眼可辨**（模块用强调色；核心保持安静） */
.card__src {
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
.card__body {
  margin: 0 0 6px;
  line-height: 1.6;
  cursor: pointer;
}
.row {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
  font-size: 11px;
  opacity: 0.85;
}
.link {
  padding: 2px 8px;
  font-size: 11px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: transparent;
  cursor: pointer;
}
.link:disabled {
  opacity: 0.5;
  cursor: default;
}
.why {
  margin: 4px 0 0;
  padding-left: 16px;
  font-size: 11px;
  opacity: 0.75;
}
</style>
