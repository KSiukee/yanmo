<script setup lang="ts">
// 「先问后排版」的那一盘：这一轮攒下的答案——排序 / 删掉 / 改字，问够了再一起落。
//
// 单独成件的原因：它是**这一轮**的托盘（与"挑哪些问题来问"、"面板长什么样"都不同），
// 而且只在一种走法下出现。它**纯展示**：动作交回面板那一层（那边才碰核心）。
import { t } from "../locales/index.ts";
import type { RoundItem } from "../api/question.ts";
import { landLabel, type LandAt } from "./question.ts";

const props = defineProps<{
  items: RoundItem[];
  busy: boolean;
  /** 作者点了「一轮问够了」：进"先看一眼再落"这一步 */
  previewing: boolean;
  /** 眼下有没有打开着的一章——没有就落不了（别放按不动的按钮） */
  canLand: boolean;
  /** 当前这一章叫什么（预览那句话要说清落到哪儿） */
  chapter: string;
  landAt: LandAt;
}>();

const emit = defineEmits<{
  move: [number, number];
  remove: [number];
  amend: [number, string];
  finish: [];
  back: [];
  clear: [];
  land: [];
}>();

/** 改字：失焦或回车之后才回写（击键级同步不跨进程，这里也不必每个字都往上报） */
function amend(index: number, event: Event) {
  emit("amend", index, (event.target as HTMLTextAreaElement).value);
}
</script>

<template>
  <section class="tray">
    <h3 class="sec">{{ t("flow.round.title") }}</h3>
    <p class="hint">{{ t("flow.round.hint") }}</p>
    <p v-if="props.items.length === 0" class="hint">{{ t("flow.round.empty") }}</p>

    <article v-for="(item, index) in props.items" :key="item.card_id" class="item">
      <textarea class="note" rows="2" :value="item.body" @change="amend(index, $event)" />
      <div class="row">
        <button class="link" :disabled="props.busy || index === 0" @click="emit('move', index, -1)">
          ↑
        </button>
        <button
          class="link"
          :disabled="props.busy || index === props.items.length - 1"
          @click="emit('move', index, 1)"
        >
          ↓
        </button>
        <button class="link" :disabled="props.busy" @click="emit('remove', index)">
          {{ t("flow.round.drop") }}
        </button>
      </div>
    </article>

    <!-- 问够了：先看一眼将要发生什么，再落 -->
    <div v-if="props.previewing && props.items.length > 0" class="foot">
      <p class="hint">
        {{
          t("flow.round.preview", {
            count: props.items.length,
            chapter: props.chapter,
            at: landLabel(props.landAt),
          })
        }}
      </p>
      <div class="row">
        <button
          class="act act--primary"
          :disabled="props.busy || !props.canLand"
          @click="emit('land')"
        >
          {{ t("flow.round.land") }}
        </button>
        <button class="act" :disabled="props.busy" @click="emit('back')">
          {{ t("flow.round.back") }}
        </button>
      </div>
      <p v-if="!props.canLand" class="hint">{{ t("flow.land.no_chapter") }}</p>
    </div>

    <div v-else-if="props.items.length > 0" class="row">
      <button class="act" :disabled="props.busy" @click="emit('finish')">
        {{ t("flow.round.finish") }}
      </button>
      <button class="link" :disabled="props.busy" @click="emit('clear')">
        {{ t("flow.round.clear") }}
      </button>
    </div>
  </section>
</template>

<style scoped>
.tray {
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
.item {
  margin-bottom: 6px;
}
.note {
  width: 100%;
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
  align-items: center;
  margin-top: 2px;
  font-size: 11px;
}
.foot {
  margin-top: 6px;
}
.act,
.link {
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
.act:disabled,
.link:disabled {
  opacity: 0.5;
  cursor: default;
}
</style>
