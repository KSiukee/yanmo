<script setup lang="ts">
// 伏笔账本：**埋下的一条线头**——一句话、埋在第几章、收了没有。
//
// 这个文件只管长什么样；状态与命令在 [`useForeshadowPanel`](./foreshadow-panel.ts)，
// 状态机的合法性在核心（`ForeshadowState::next_states`）。
//
// 两句要说清的话：
// - **「不写了」是正经结局**，不是失败（界面上也不说"未完成"）；
// - 「收了」会把"收在哪一章"记成**你现在打开的这一章**（作者多半就站在收线那一章上）。
import { t } from "../locales/index.ts";
import type { Foreshadow, ForeshadowState } from "../api/foreshadow.ts";
import type { EditorSession } from "../editor/session.ts";
import { anchorLabel, stateLabel, whereLabel } from "./foreshadow.ts";

const props = defineProps<{ session: EditorSession }>();
const { workId, directory } = props.session;
const {
  board,
  busy,
  errorText,
  filter,
  draft,
  justSaved,
  options,
  list,
  canSave,
  startEdit,
  cancelEdit,
  save,
  move,
  remove,
} = props.session.foreshadows;

/** 跳到这一章：先在树上露出来（卷可能收着），再切过去。 */
async function goChapter(node_id: number) {
  await directory.reveal(node_id);
  await directory.select(node_id);
}

function askRemove(item: Foreshadow) {
  if (window.confirm(t("foreshadow.delete_confirm"))) void remove(item);
}

/** 走一步状态时按钮上写什么（与核心的迁移表同源）。 */
function stateAction(state: ForeshadowState): string {
  if (state === "collected") return t("foreshadow.collect");
  if (state === "dropped") return t("foreshadow.drop");
  return t("foreshadow.replant");
}

/** 这条伏笔能走到的下一步（`planted` 给两个按钮，别的给"又埋着"）。 */
const NEXT: Record<ForeshadowState, ForeshadowState[]> = {
  planted: ["collected", "dropped"],
  collected: ["planted"],
  dropped: ["planted"],
};
</script>

<template>
  <section class="fores">
    <p class="fores__rule">{{ t("foreshadow.rule") }}</p>
    <p v-if="errorText" class="fores__error">{{ errorText }}</p>
    <p v-if="justSaved" class="fores__saved">{{ justSaved }}</p>
    <p v-if="workId === null" class="fores__hint">{{ t("entity.no_work") }}</p>

    <!-- 表单：记一条 / 改一条（状态不在这儿改，走列表上那几个按钮） -->
    <form v-if="draft" class="fores__form" @submit.prevent="void save()">
      <label class="fores__field">
        <span class="fores__label">{{ t("foreshadow.body") }}</span>
        <input
          v-model="draft.body"
          class="fores__input"
          :placeholder="t('foreshadow.body_placeholder')"
          :disabled="busy"
        />
      </label>
      <label class="fores__field">
        <span class="fores__label">{{ t("foreshadow.planted") }}</span>
        <select v-model="draft.planted_node" class="fores__input" :disabled="busy">
          <option :value="null">{{ t("foreshadow.chapter_none") }}</option>
          <option v-for="chapter in board?.chapters ?? []" :key="chapter.id" :value="chapter.id">
            {{ chapter.title }}
          </option>
        </select>
      </label>
      <label class="fores__field">
        <span class="fores__label">{{ t("foreshadow.note") }}</span>
        <input v-model="draft.note" class="fores__input" :disabled="busy" />
      </label>
      <div class="fores__actions">
        <button type="submit" class="fores__button" :disabled="busy || !canSave">
          {{ t("foreshadow.save") }}
        </button>
        <button type="button" class="fores__button" :disabled="busy" @click="cancelEdit()">
          {{ t("foreshadow.cancel") }}
        </button>
      </div>
    </form>

    <nav v-if="board" class="fores__filters">
      <button
        v-for="option in options"
        :key="option.state"
        type="button"
        class="fores__chip"
        :class="{ 'fores__chip--on': filter === option.state }"
        :disabled="busy"
        @click="filter = option.state"
      >
        {{ option.state === "all" ? t("foreshadow.filter.all") : stateLabel(option.state) }}
        <span class="fores__n">{{ t("foreshadow.count", { count: option.count }) }}</span>
      </button>
    </nav>

    <p v-if="busy && !board" class="fores__hint">{{ t("foreshadow.loading") }}</p>
    <ul v-else-if="list.length > 0" class="fores__list">
      <li v-for="item in list" :key="item.id" class="fores__item">
        <p class="fores__head">
          <span class="fores__state" :class="`fores__state--${item.state}`">
            {{ stateLabel(item.state) }}
          </span>
          <span class="fores__body">{{ item.body }}</span>
          <span v-if="whereLabel(board?.chapters ?? [], item)" class="fores__where">
            {{ whereLabel(board?.chapters ?? [], item) }}
          </span>
          <span v-if="anchorLabel(board?.chapters ?? [], item.collected_node, 'collected')" class="fores__where">
            {{ anchorLabel(board?.chapters ?? [], item.collected_node, "collected") }}
          </span>
        </p>
        <p v-if="item.note" class="fores__note">{{ item.note }}</p>
        <div class="fores__row">
          <button
            v-for="next in NEXT[item.state]"
            :key="next"
            type="button"
            class="fores__link"
            :disabled="busy"
            @click="void move(item, next)"
          >
            {{ stateAction(next) }}
          </button>
          <button
            v-if="item.planted_node !== null"
            type="button"
            class="fores__link"
            :disabled="busy"
            @click="void goChapter(item.planted_node)"
          >
            {{ t("foreshadow.goto") }}
          </button>
          <button type="button" class="fores__link" :disabled="busy" @click="startEdit(item)">
            {{ t("foreshadow.edit") }}
          </button>
          <button type="button" class="fores__link" :disabled="busy" @click="askRemove(item)">
            {{ t("foreshadow.delete") }}
          </button>
        </div>
      </li>
    </ul>
    <p v-else-if="board && !busy" class="fores__hint">{{ t("foreshadow.empty") }}</p>
  </section>
</template>

<style scoped>
.fores {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: auto;
}

.fores__rule,
.fores__hint {
  margin: 0 0 8px;
  font-size: 12px;
  line-height: 1.6;
  opacity: 0.75;
}

.fores__error {
  margin: 0 0 8px;
  font-size: 12px;
  color: var(--ym-danger, #c0392b);
}

.fores__saved {
  margin: 0 0 8px;
  font-size: 12px;
  opacity: 0.8;
}

.fores__form {
  display: grid;
  gap: 6px;
  margin: 0 0 10px;
  padding: 10px;
  border: 1px solid var(--ym-line);
  border-radius: 6px;
  background: var(--ym-paper-dim);
}

.fores__field {
  display: grid;
  grid-template-columns: 110px minmax(0, 1fr);
  gap: 8px;
  align-items: center;
}

.fores__label {
  font-size: 12px;
  opacity: 0.8;
}

.fores__input {
  box-sizing: border-box;
  width: 100%;
  padding: 3px 6px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: var(--ym-paper);
  color: inherit;
  font: inherit;
  font-size: 13px;
}

.fores__actions {
  display: flex;
  gap: 6px;
  justify-content: flex-end;
}

.fores__button,
.fores__link {
  padding: 2px 10px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: var(--ym-paper);
  color: inherit;
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}

.fores__button:disabled,
.fores__link:disabled {
  opacity: 0.5;
  cursor: default;
}

.fores__filters {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  margin: 0 0 8px;
}

.fores__chip {
  padding: 2px 8px;
  border: 1px solid var(--ym-line);
  border-radius: 999px;
  background: transparent;
  color: inherit;
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}

.fores__chip--on {
  border-color: var(--ym-accent);
  color: var(--ym-accent);
}

.fores__n {
  margin-left: 4px;
  opacity: 0.7;
}

.fores__list {
  margin: 0;
  padding: 0;
  list-style: none;
}

.fores__item {
  margin: 0 0 8px;
  padding: 0 0 8px;
  border-bottom: 1px solid var(--ym-line);
}

.fores__head {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: baseline;
  margin: 0 0 2px;
  font-size: 12px;
}

.fores__state {
  padding: 0 4px;
  border: 1px solid var(--ym-line);
  border-radius: 3px;
  font-size: 10px;
}

.fores__state--planted {
  border-color: var(--ym-accent);
  color: var(--ym-accent);
}

.fores__state--dropped {
  opacity: 0.6;
}

.fores__body {
  font-weight: 600;
}

.fores__where,
.fores__note {
  font-size: 11px;
  opacity: 0.7;
}

.fores__note {
  margin: 0 0 4px;
}

.fores__row {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}
</style>
