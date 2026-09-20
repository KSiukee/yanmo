<script setup lang="ts">
// 「碰撞」这一片：**机制挑两条摆在一起，问一句"它们之间是什么关系"**。
//
// 关键一条：**机制一个字都不生成**——上面那句引子是按关系模板渲染出来的骨架
// （两条原文都是你自己写的），撞出来的那一刻由你在下面那个框里写；
// 写下的那句存成一张普通灵感卡，带着**双源溯源**（哪两条撞出来的永远查得到）。
//
// 这一块只管长什么样：挑对、存下、跳过全在 [`useCollidePanel`](./collide-panel.ts)。
import { onMounted, watch } from "vue";

import type { EditorSession } from "../editor/session.ts";
import { t } from "../locales/index.ts";
import { excerpt, pairKey, relationLabel, sideLabel, sideWhenText } from "./collide.ts";

const props = defineProps<{ session: EditorSession }>();

const {
  board,
  busy,
  errorText,
  receipt,
  lastSkipped,
  currentChapter,
  refresh,
  draftFor,
  setDraft,
  save,
  skip,
  undoSkip,
} = props.session.collide;

// **按需读**：这一片挂上才读一次；作者切了章再读一次（挑对跟着"现在写到哪儿"走）
onMounted(() => void refresh());
watch(currentChapter, () => void refresh());
</script>

<template>
  <section class="collide">
    <p class="hint">{{ t("collide.rule") }}</p>
    <p v-if="errorText" class="error">{{ errorText }}</p>

    <!-- 回执与那一手撤销：跳过是落盘的，所以"撤销"当场就能兑现 -->
    <section v-if="receipt || lastSkipped !== null" class="receipt">
      <p v-if="receipt" class="saved">{{ receipt }}</p>
      <p v-if="lastSkipped !== null" class="saved">
        {{ t("collide.skipped") }}
        <button class="link" :disabled="busy" @click="void undoSkip()">
          {{ t("creator.undo") }}
        </button>
      </p>
    </section>

    <ul v-if="board.length > 0" class="pairs">
      <li v-for="pair in board" :key="pairKey(pair)" class="pair">
        <p class="pair__why">{{ relationLabel(pair.relation) }}</p>
        <!-- 两头可能是碎片，也可能是大纲元素：种类徽标说清撞的是什么 -->
        <template v-for="side in [pair.left, pair.right]" :key="side.key">
          <p class="pair__side">
            <span class="badge">{{ sideLabel(side.kind) }}</span>
            <span v-if="sideWhenText(side)" class="pair__when">{{ sideWhenText(side) }}</span>
          </p>
          <p class="pair__body">{{ excerpt(side.text) }}</p>
          <p v-if="side.note" class="pair__note">{{ side.note }}</p>
        </template>

        <!-- 引子只是骨架：这句问话可以改、也可以删光了重写（引子里嵌的是你自己的两条原文） -->
        <textarea
          class="pair__draft"
          rows="3"
          :value="draftFor(pair)"
          :disabled="busy"
          @input="setDraft(pair, ($event.target as HTMLTextAreaElement).value)"
        ></textarea>
        <p class="row">
          <button class="link" :disabled="busy" @click="void save(pair)">
            {{ t("collide.save") }}
          </button>
          <button class="link" :disabled="busy" @click="void skip(pair)">
            {{ t("collide.skip") }}
          </button>
        </p>
      </li>
    </ul>
    <p v-else-if="!busy" class="hint">{{ t("collide.empty") }}</p>
  </section>
</template>

<style scoped>
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

.receipt {
  margin: 0 0 6px;
}

.saved {
  margin: 0 0 4px;
  font-size: 11px;
  opacity: 0.8;
}

.pairs {
  margin: 0;
  padding: 0;
  list-style: none;
}

.pair {
  margin: 0 0 10px;
  padding: 0 0 10px;
  border-bottom: 1px solid var(--ym-line);
}

.pair__why {
  margin: 0 0 4px;
  font-size: 11px;
  font-weight: 600;
  opacity: 0.85;
}

.pair__side {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  align-items: center;
  margin: 0 0 2px;
  font-size: 11px;
  opacity: 0.8;
}

.pair__when {
  margin-left: auto;
}

.badge {
  padding: 0 4px;
  border: 1px solid var(--ym-line);
  border-radius: 3px;
  font-size: 10px;
}

.pair__body {
  margin: 0 0 4px;
  font-size: 12px;
  line-height: 1.5;
  word-break: break-word;
}

.pair__note {
  margin: 0 0 4px;
  font-size: 11px;
  opacity: 0.7;
}

.pair__draft {
  box-sizing: border-box;
  width: 100%;
  margin: 0 0 6px;
  padding: 4px 6px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: var(--ym-paper);
  color: inherit;
  font: inherit;
  font-size: 12px;
  resize: vertical;
}

.row {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
  margin: 0;
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

.link:disabled {
  opacity: 0.5;
  cursor: default;
}
</style>
