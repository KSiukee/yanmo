<script setup lang="ts">
// 「大纲」面板里**事件**那一页：什么时候、发生了什么。
//
// 它与「创作流 → 碎片池」是**同一份数据**（碎片统一表里 `frag_kind = 'event'`），
// 所以这一页直接用那一份状态（`session.creator`）——两处各读一份迟早对不上。
//
// 三句要说清的话：
// - 事件记下来时**顺手带上"是在这一章写的"**（锚点），"故事时间"是另一回事、另填；
// - **故事时间填得少就报得少**：只有"两边都填了数字、都不是倒叙"才判得动顺序；
// - 倒叙 / 回忆是作者自己的写法，勾上就不参与顺序检查。
import { ref } from "vue";

import type { EditorSession } from "../editor/session.ts";
import { t } from "../locales/index.ts";
import { storyLabel } from "./creator.ts";
import EventEdit from "./EventEdit.vue";

const props = defineProps<{ session: EditorSession }>();
const { workId, directory } = props.session;
const { board, busy, errorText, justSaved, editing, startEdit, cancelEdit, saveEdit, jot } =
  props.session.creator;

/** 这一页只看事件（别的种类归创作流那一栏）。 */
const events = () => (board.value?.fragments ?? []).filter((item) => item.kind === "event");

const draft = ref("");

async function add() {
  if (await jot(draft.value, "event")) draft.value = "";
}

/** 跳到写这条事件的那一章。 */
async function goChapter(anchors: string[]) {
  const anchor = anchors.find((item) => item.startsWith("chapter:"));
  const node_id = anchor ? Number(anchor.split(":")[1]) : Number.NaN;
  if (!Number.isFinite(node_id)) return;
  await directory.reveal(node_id);
  await directory.select(node_id);
}

function chapterOf(anchors: string[]): number | null {
  const anchor = anchors.find((item) => item.startsWith("chapter:"));
  const node_id = anchor ? Number(anchor.split(":")[1]) : Number.NaN;
  return Number.isFinite(node_id) ? node_id : null;
}
</script>

<template>
  <section class="events">
    <p class="events__rule">{{ t("event.rule") }}</p>
    <p v-if="errorText" class="events__error">{{ errorText }}</p>
    <p v-if="justSaved" class="events__saved">{{ justSaved }}</p>
    <p v-if="workId === null" class="events__hint">{{ t("entity.no_work") }}</p>

    <form class="events__jot" @submit.prevent="void add()">
      <input
        v-model="draft"
        class="events__input"
        :placeholder="t('event.placeholder')"
        :disabled="busy"
      />
      <button type="submit" class="events__button" :disabled="busy || !draft.trim()">
        {{ t("event.add") }}
      </button>
    </form>
    <p class="events__hint">{{ t("event.hint") }}</p>

    <p v-if="busy && !board" class="events__hint">{{ t("event.loading") }}</p>
    <ul v-else-if="events().length > 0" class="events__list">
      <li v-for="item in events()" :key="item.id" class="events__item">
        <p class="events__head">
          <span class="events__body">{{ item.body }}</span>
          <span v-if="storyLabel(item)" class="events__badge">{{ storyLabel(item) }}</span>
          <span v-if="item.flashback" class="events__badge">{{ t("creator.badge.flashback") }}</span>
        </p>
        <div class="events__row">
          <button
            v-if="chapterOf(item.anchors) !== null"
            type="button"
            class="events__link"
            :disabled="busy"
            @click="void goChapter(item.anchors)"
          >
            {{ t("event.goto") }}
          </button>
          <button type="button" class="events__link" :disabled="busy" @click="startEdit(item)">
            {{ t("creator.edit") }}
          </button>
        </div>
        <EventEdit
          v-if="editing && editing.id === item.id"
          :session="session"
          :editing="editing"
          :busy="busy"
          @save="void saveEdit()"
          @cancel="cancelEdit()"
        />
      </li>
    </ul>
    <p v-else-if="board && !busy" class="events__hint">{{ t("event.empty") }}</p>
  </section>
</template>

<style scoped>
.events {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: auto;
}

.events__rule,
.events__hint {
  margin: 0 0 8px;
  font-size: 12px;
  line-height: 1.6;
  opacity: 0.75;
}

.events__error {
  margin: 0 0 8px;
  font-size: 12px;
  color: var(--ym-danger, #c0392b);
}

.events__saved {
  margin: 0 0 8px;
  font-size: 12px;
  opacity: 0.8;
}

.events__jot {
  display: flex;
  gap: 6px;
  margin: 0 0 4px;
}

.events__input {
  box-sizing: border-box;
  flex: 1;
  min-width: 0;
  padding: 3px 6px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: var(--ym-paper);
  color: inherit;
  font: inherit;
  font-size: 13px;
}

.events__button,
.events__link {
  padding: 2px 10px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: var(--ym-paper);
  color: inherit;
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}

.events__button:disabled,
.events__link:disabled {
  opacity: 0.5;
  cursor: default;
}

.events__list {
  margin: 0;
  padding: 0;
  list-style: none;
}

.events__item {
  margin: 0 0 8px;
  padding: 0 0 8px;
  border-bottom: 1px solid var(--ym-line);
}

.events__head {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: baseline;
  margin: 0 0 4px;
  font-size: 13px;
}

.events__body {
  font-weight: 600;
}

.events__badge {
  padding: 0 4px;
  border: 1px solid var(--ym-line);
  border-radius: 3px;
  font-size: 10px;
  opacity: 0.85;
}

.events__row {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}
</style>
