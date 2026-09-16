<script setup lang="ts">
// 「大纲」面板里**场景卡**那一页：一眼看全书哪几张还没填，就地补上四格。
//
// 与编辑器里那张小表单的分工：**写的时候**在编辑器顺手填（那张卡正开着）；
// **回头检查的时候**在这儿看全貌（哪一场缺什么，一处补齐）。
//
// 三句要说清的话：
// - 场景卡本身就是目录树上一个节点（挂在某一章下面），名字与正文照旧在树上；
//   这里只管它**结构化的那四格**；
// - 缺项只报告、绝不代写（"这句算不算目标"是作者的事，这里只问填没填）；
// - 四格是**主动场景**那一套（视角 / 目标 / 冲突 / 结果）。
import { computed, reactive, ref } from "vue";

import { t } from "../locales/index.ts";
import type { SceneFields } from "../api/core.ts";
import type { EditorSession } from "../editor/session.ts";

const props = defineProps<{ session: EditorSession }>();
const { workId, directory } = props.session;
const { cards, busy, errorText, justSaved, load, save } = props.session.scenes;

/** 四格的顺序（与核心 `SceneField::ALL` 一致）。 */
const FIELDS = ["pov", "goal", "conflict", "outcome"] as const;

/** 每张卡手上那一份草稿（键＝节点 id）：改哪张填哪张，不整屏重存。 */
const drafts = reactive<Record<number, SceneFields>>({});

/** 手上这份草稿（没有就从库里那份起头）。 */
function draftOf(card: { node_id: number; fields: SceneFields }): SceneFields {
  if (!drafts[card.node_id]) drafts[card.node_id] = { ...card.fields };
  return drafts[card.node_id];
}

/** 这一张还缺几格（判据只有一条：修剪后是不是空的——与核心同一条口径）。 */
function missing(fields: SceneFields): number {
  return FIELDS.filter((field) => (fields[field] ?? "").trim() === "").length;
}

/** 还没填全的那些排前面（要看的先看得见），填全了的按树里的顺序跟在后面。 */
const ordered = computed(() =>
  [...cards.value].sort((a, b) => missing(draftOf(b)) - missing(draftOf(a))),
);

async function store(node_id: number) {
  await save(draftOf({ node_id, fields: drafts[node_id] }));
}

/** 跳到那一场（先在树上露出来，再切过去）。 */
async function goScene(node_id: number) {
  await directory.reveal(node_id);
  await directory.select(node_id);
}

// ── 新建一张：挂在**当前这一章**下面（作者多半就想在当前章加一场） ──
const newTitle = ref("");
async function createScene() {
  const parent = directory.current.value;
  if (parent === null || !newTitle.value.trim()) return;
  const id = await directory.create(parent, "scene", newTitle.value.trim());
  if (id !== null) {
    newTitle.value = "";
    await load();
  }
}
</script>

<template>
  <section class="scenes">
    <p class="scenes__rule">{{ t("scene.rule") }}</p>
    <p v-if="errorText" class="scenes__error">{{ errorText }}</p>
    <p v-if="workId === null" class="scenes__hint">{{ t("entity.no_work") }}</p>

    <form class="scenes__new" @submit.prevent="void createScene()">
      <input
        v-model="newTitle"
        class="scenes__input"
        :placeholder="t('scene.new_placeholder')"
        :disabled="busy || directory.current.value === null"
      />
      <button
        type="submit"
        class="scenes__button"
        :disabled="busy || !newTitle.trim() || directory.current.value === null"
      >
        {{ t("scene.new") }}
      </button>
    </form>
    <p class="scenes__hint">
      {{ directory.current.value === null ? t("scene.no_chapter") : t("scene.new_hint") }}
    </p>

    <p v-if="busy && cards.length === 0" class="scenes__hint">{{ t("scene.loading") }}</p>
    <ul v-else-if="cards.length > 0" class="scenes__list">
      <li v-for="card in ordered" :key="card.node_id" class="scenes__item">
        <p class="scenes__head">
          <span class="scenes__title">
            {{ card.title.trim() === "" ? t("scene.untitled") : card.title }}
          </span>
          <span v-if="missing(draftOf(card)) > 0" class="scenes__badge">
            {{ t("scene.missing", { count: missing(draftOf(card)) }) }}
          </span>
          <span v-else class="scenes__badge scenes__badge--done">{{ t("scene.done") }}</span>
        </p>
        <div class="scenes__grid">
          <label v-for="field in FIELDS" :key="field" class="scenes__field">
            <span class="scenes__label">{{ t(`outline.field.${field}`) }}</span>
            <input
              v-model="draftOf(card)[field]"
              class="scenes__input"
              type="text"
              :disabled="busy"
            />
          </label>
        </div>
        <div class="scenes__row">
          <button type="button" class="scenes__button" :disabled="busy" @click="void store(card.node_id)">
            {{ t("scene.save") }}
          </button>
          <button type="button" class="scenes__link" :disabled="busy" @click="void goScene(card.node_id)">
            {{ t("scene.goto") }}
          </button>
          <span v-if="justSaved === card.node_id" class="scenes__hint">{{ t("scene.saved") }}</span>
        </div>
      </li>
    </ul>
    <p v-else-if="!busy" class="scenes__hint">{{ t("scene.empty") }}</p>
  </section>
</template>

<style scoped>
.scenes {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: auto;
}

.scenes__rule,
.scenes__hint {
  margin: 0 0 8px;
  font-size: 12px;
  line-height: 1.6;
  opacity: 0.75;
}

.scenes__error {
  margin: 0 0 8px;
  font-size: 12px;
  color: var(--ym-danger, #c0392b);
}

.scenes__new {
  display: flex;
  gap: 6px;
  margin: 0 0 4px;
}

.scenes__input {
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

.scenes__button,
.scenes__link {
  flex: none;
  padding: 2px 10px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: var(--ym-paper);
  color: inherit;
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}

.scenes__link {
  background: transparent;
}

.scenes__button:disabled,
.scenes__link:disabled {
  opacity: 0.5;
  cursor: default;
}

.scenes__list {
  margin: 0;
  padding: 0;
  list-style: none;
}

.scenes__item {
  margin: 0 0 10px;
  padding: 0 0 8px;
  border-bottom: 1px solid var(--ym-line);
}

.scenes__head {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: baseline;
  margin: 0 0 4px;
  font-size: 13px;
}

.scenes__title {
  font-weight: 600;
}

.scenes__badge {
  padding: 0 4px;
  border: 1px solid var(--ym-line);
  border-radius: 3px;
  font-size: 10px;
  color: var(--ym-accent);
}

.scenes__badge--done {
  color: var(--ym-ink-soft);
}

.scenes__grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 4px 10px;
  margin: 0 0 6px;
}

.scenes__field {
  display: flex;
  gap: 6px;
  align-items: center;
  min-width: 0;
}

.scenes__label {
  flex: none;
  width: 3em;
  font-size: 11px;
  opacity: 0.7;
}

.scenes__row {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
}
</style>
