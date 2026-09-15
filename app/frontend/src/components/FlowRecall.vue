<script setup lang="ts">
// 回头路：**已经放下的东西能捡回来**——冷却库（舍弃不等于删除）与两张已静音清单。
//
// 单独成件的原因：它与"挑问题、处置、作答"那半边的变化理由不一样——
// 那边在长交互，这边只是在长"撤销入口"（面板因此不再一路变长）。
// 它**纯展示**：动作交回给面板那一层去调命令，免得同一条命令有两个调用点。
import { t } from "../locales/index.ts";
import type { CooledCard } from "../api/question.ts";
import { classLabel, sourceLabel } from "./question.ts";

const props = defineProps<{
  busy: boolean;
  /** 冷却库：舍弃过的卡（可捞回） */
  cooled: CooledCard[];
  /** 已静音的**类别**（模板键；界面按字典渲染成"这类别再问"） */
  mutedClasses: string[];
  mutedSources: string[];
}>();

const emit = defineEmits<{
  retrieve: [number];
  muteSource: [string];
  unmuteClass: [string];
  unmuteSource: [string];
}>();
</script>

<template>
  <!-- 冷却库：舍弃不等于删除 -->
  <section v-if="props.cooled.length > 0">
    <h3 class="sec">{{ t("flow.cooled") }}</h3>
    <p class="hint">{{ t("flow.cooled.hint") }}</p>
    <article v-for="item in props.cooled" :key="item.card_id" class="card">
      <p class="card__dim">{{ item.body }}</p>
      <div class="row">
        <span class="src">{{ sourceLabel(item.source) }}</span>
        <button class="link" :disabled="props.busy" @click="emit('retrieve', item.card_id)">
          {{ t("flow.action.retrieve") }}
        </button>
        <button class="link" :disabled="props.busy" @click="emit('muteSource', item.source)">
          {{ t("flow.source.mute") }}
        </button>
      </div>
    </article>
  </section>

  <!-- 已静音的类别：「这类别再问」的回头路 -->
  <section v-if="props.mutedClasses.length > 0">
    <h3 class="sec">{{ t("flow.muted_classes") }}</h3>
    <p class="hint">{{ t("flow.muted_classes.hint") }}</p>
    <div class="row">
      <span v-for="key in props.mutedClasses" :key="key" class="chip">
        {{ classLabel(key) }}
        <button class="link" :disabled="props.busy" @click="emit('unmuteClass', key)">✕</button>
      </span>
    </div>
  </section>

  <!-- 已静音的来源：能一键让它闭嘴，也能解除 -->
  <section v-if="props.mutedSources.length > 0">
    <h3 class="sec">{{ t("flow.muted_sources") }}</h3>
    <div class="row">
      <span v-for="source in props.mutedSources" :key="source" class="chip">
        {{ sourceLabel(source) }}
        <button class="link" :disabled="props.busy" @click="emit('unmuteSource', source)">✕</button>
      </span>
    </div>
  </section>
</template>

<style scoped>
.sec {
  margin: 12px 0 6px;
  font-size: 11px;
  opacity: 0.75;
}
.hint {
  margin: 0 0 8px;
  font-size: 11px;
  line-height: 1.5;
  opacity: 0.7;
}
.card {
  padding: 8px;
  margin-bottom: 6px;
  border: 1px solid var(--ym-line);
  border-radius: 6px;
  background: var(--ym-paper);
}
.card__dim {
  margin: 0 0 6px;
  line-height: 1.6;
  opacity: 0.65;
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
.chip {
  display: inline-flex;
  gap: 4px;
  align-items: center;
  padding: 1px 6px;
  font-size: 11px;
  border: 1px solid var(--ym-line);
  border-radius: 10px;
}
</style>
