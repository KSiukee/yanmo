<script setup lang="ts">
// 「灵感」这一片：**机制替你把这堆碎片排个序**，采纳到哪儿由你定。
//
// 与「碎片」那一片同一张表、同一批碎片（所以同住创作流这一栏）：那一片管"记 / 看 / 删"，
// 这一片管"排 / 采纳"。关键一条：**这里一个字都不生成**——作者看到的全是他自己写过的句子，
// 只是被排了序，而且每一句都摆着"凭什么排在前面"（四项乘数）。
//
// 这一块只管长什么样：排序、采纳、丢弃、撤销全在 [`useInspirePanel`](./inspire-panel.ts)。
import { onMounted, watch } from "vue";

import type { EditorSession } from "../editor/session.ts";
import { formatWhen } from "../editor/display.ts";
import { t } from "../locales/index.ts";
import { anchoredToChapter, kindLabel, sourceBadge } from "./creator.ts";
import { gravityLabel, targetLabel } from "./inspire.ts";

const props = defineProps<{ session: EditorSession }>();

const {
  board,
  busy,
  errorText,
  receipt,
  target,
  targets,
  list,
  needsChapter,
  canAdopt,
  currentChapter,
  lastDiscarded,
  refresh,
  setTarget,
  adopt,
  discard,
  undoDiscard,
} = props.session.inspire;

// **按需读**：这一片挂上才读一次；作者切了章再读一次（排序跟着"现在写到哪儿"走）。
// 不打开这一片的人，不为它付一次读库。
onMounted(() => void refresh());
watch(currentChapter, () => void refresh());
</script>

<template>
  <section class="inspire">
    <p class="hint">{{ t("inspire.rule") }}</p>
    <p v-if="errorText" class="error">{{ errorText }}</p>

    <!-- 采纳到哪儿：落点是**核心给的闭集**（界面不自己写一份） -->
    <div class="row">
      <span class="hint">{{ t("inspire.target") }}</span>
      <button
        v-for="option in targets"
        :key="option"
        type="button"
        class="chip"
        :class="{ 'chip--on': target === option }"
        :disabled="busy"
        @click="setTarget(option)"
      >
        {{ targetLabel(option) }}
      </button>
    </div>
    <p v-if="needsChapter && !canAdopt" class="hint">{{ t("inspire.need_chapter") }}</p>

    <!-- 回执与那一手撤销：丢弃是软删，所以"撤销"当场就能兑现 -->
    <section v-if="receipt || lastDiscarded !== null" class="receipt">
      <p v-if="receipt" class="saved">{{ receipt }}</p>
      <p v-if="lastDiscarded !== null" class="saved">
        {{ t("inspire.discarded") }}
        <button class="link" :disabled="busy" @click="void undoDiscard()">
          {{ t("creator.undo") }}
        </button>
      </p>
    </section>

    <p v-if="busy && !board" class="hint">{{ t("inspire.loading") }}</p>
    <ul v-else-if="list.length > 0" class="sparks">
      <li v-for="item in list" :key="item.id" class="spark">
        <p class="spark__head">
          <span class="spark__kind">{{ kindLabel(item.kind) }}</span>
          <span v-if="anchoredToChapter(item.anchors, currentChapter)" class="badge">
            {{ t("creator.badge.this_chapter") }}
          </span>
          <span v-if="sourceBadge(item.source)" class="badge">
            {{ sourceBadge(item.source) }}
          </span>
          <span v-if="item.used_count > 0" class="badge">
            {{ t("inspire.used", { count: item.used_count }) }}
          </span>
          <span class="spark__when">{{ formatWhen(item.created_at) }}</span>
        </p>
        <p class="spark__body">{{ item.body }}</p>
        <!-- 为什么它在前面：四项乘数都摆出来（核心算的，界面只显示） -->
        <p class="spark__why">{{ gravityLabel(item.gravity) }}</p>
        <p class="row">
          <button class="link" :disabled="busy || !canAdopt" @click="void adopt(item)">
            {{ t("inspire.adopt") }}
          </button>
          <button class="link" :disabled="busy" @click="void discard(item)">
            {{ t("inspire.discard") }}
          </button>
        </p>
      </li>
    </ul>
    <p v-else-if="board && !busy" class="hint">{{ t("inspire.empty") }}</p>
  </section>
</template>

<style scoped>
.inspire {
  margin: 0 0 8px;
}

.hint {
  margin: 0 0 6px;
  font-size: 11px;
  line-height: 1.5;
  opacity: 0.7;
}

.error {
  margin: 0 0 6px;
  font-size: 11px;
  color: var(--ym-danger);
}

.row {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  align-items: center;
  margin: 0 0 6px;
}

.chip {
  padding: 2px 8px;
  border: 1px solid var(--ym-line);
  border-radius: 999px;
  background: transparent;
  color: inherit;
  font: inherit;
  font-size: 11px;
  cursor: pointer;
}

.chip--on {
  border-color: var(--ym-accent);
  color: var(--ym-accent);
}

.link {
  padding: 2px 8px;
  font-size: 11px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: transparent;
  color: inherit;
  cursor: pointer;
}

.link:disabled,
.chip:disabled {
  opacity: 0.5;
  cursor: default;
}

.receipt {
  margin: 0 0 6px;
}

.saved {
  margin: 0 0 4px;
  font-size: 11px;
  opacity: 0.8;
}

.sparks {
  margin: 0;
  padding: 0;
  list-style: none;
}

.spark {
  margin: 0 0 8px;
  padding: 0 0 8px;
  border-bottom: 1px solid var(--ym-line);
}

.spark__head {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  align-items: center;
  margin: 0 0 2px;
  font-size: 11px;
  opacity: 0.8;
}

.spark__kind {
  font-weight: 600;
  opacity: 1;
}

.badge {
  padding: 0 4px;
  border: 1px solid var(--ym-line);
  border-radius: 3px;
  font-size: 10px;
}

.spark__when {
  margin-left: auto;
}

.spark__body {
  margin: 0 0 2px;
  font-size: 12px;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-word;
}

.spark__why {
  margin: 0 0 4px;
  font-size: 10px;
  opacity: 0.6;
}
</style>
