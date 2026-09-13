<script setup lang="ts">
// 书架：**多作品是默认形态**——这一屏就是"我手上有哪几本书、各写到什么程度"。
//
// 只在打开时拉一次列表（书架不是常驻画面），切书交给会话层（先落盘再切）。
import { nextTick, ref } from "vue";

import type { EditorSession } from "../editor/session";
import { shelfKindLabel, shelfLabel } from "../editor/shelf";
import { t } from "../locales/index.ts";
import { formatWhen } from "../editor/display";

const props = defineProps<{ session: EditorSession }>();
const {
  entries,
  busy,
  close,
  open,
  create,
  rename,
  remove,
  export: exportWork,
  saveSummary,
  note,
} = props.session.shelf;
const { visible: trashVisible, toggle: toggleTrash } = props.session.trash;
const { workId, caliber } = props.session;

/** 去回收站：先把书架收起来，免得两层弹层叠在一起 */
function openTrash() {
  close();
  toggleTrash();
}

/** 正在改名的那一本（同时只可能有一本） */
const renaming = ref<number | null>(null);
const draft = ref("");

/** 正在写简介的那一本（投稿包的大纲要用它）：多行，所以跟改名的单行输入分开 */
const noting = ref<number | null>(null);
const noteDraft = ref("");
async function startNote(work_id: number, summary: string) {
  noting.value = work_id;
  noteDraft.value = summary;
  await nextTick();
  const box = listEl.value?.querySelector<HTMLTextAreaElement>(".shelf__summary");
  box?.focus();
}

/** 存简介：**存下了才收起来**（存不下去就留在框里，别让作者白写） */
async function commitNote(work_id: number) {
  if (noting.value !== work_id) return;
  noting.value = null;
  await saveSummary(work_id, noteDraft.value);
}
const listEl = ref<HTMLElement | null>(null);

/** 新建那本的表单 */
const creating = ref(false);
const newTitle = ref("");
const newKind = ref("novel");
const newTitleEl = ref<HTMLInputElement | null>(null);

const KINDS = [
  { value: "novel", label: "shelf.kind_novel" },
  { value: "collection", label: "shelf.kind_collection" },
  { value: "article", label: "shelf.kind_article" },
];

/** 书架上的书名：没起名的那一本显示占位（默认名不落库，名字由作者起） */
function workLabel(title: string): string {
  return title || t("shelf.untitled_work");
}

async function startCreate() {
  creating.value = true;
  newTitle.value = "";
  await nextTick();
  newTitleEl.value?.focus();
}

async function submitCreate() {
  const title = newTitle.value.trim();
  if (!title) return;
  creating.value = false;
  await create(newKind.value, title);
}

async function startRename(work_id: number, title: string) {
  renaming.value = work_id;
  draft.value = title;
  await nextTick();
  const input = listEl.value?.querySelector<HTMLInputElement>(".shelf__rename");
  input?.focus();
  input?.select();
}

async function commitRename(work_id: number) {
  if (renaming.value !== work_id) return;
  renaming.value = null;
  await rename(work_id, draft.value);
}

/** 删书是不可逆的入口（虽然库里是软删），问一句再动手 */
function confirmRemove(work_id: number, title: string) {
  if (window.confirm(t("shelf.delete_confirm", { title: workLabel(title) }))) {
    void remove(work_id);
  }
}
</script>

<template>
  <div class="shelf dialog" @click.self="close">
    <section class="shelf__box dialog__box">
      <header class="shelf__head dialog__head">
        <h2 class="shelf__title dialog__title">{{ t("shelf.title") }}</h2>
        <button type="button" class="shelf__button dialog__button" :disabled="busy" @click="startCreate">
          {{ t("shelf.new_work") }}
        </button>
        <button type="button" class="shelf__button dialog__button" :title="t('shelf.trash_title')" @click="openTrash">
          {{ t("shelf.trash") }}
        </button>
        <button type="button" class="shelf__button dialog__button" :title="t('shelf.close_title')" @click="close">{{ t("common.close") }}</button>
      </header>

      <form v-if="creating" class="shelf__new" @submit.prevent="submitCreate">
        <input
          ref="newTitleEl"
          v-model="newTitle"
          class="shelf__input"
          type="text"
          :placeholder="t('shelf.name_placeholder')"
          @keydown.esc="creating = false"
        />
        <select v-model="newKind" class="shelf__kind">
          <option v-for="kind in KINDS" :key="kind.value" :value="kind.value">
            {{ t(kind.label) }}
          </option>
        </select>
        <button type="submit" class="shelf__button dialog__button" :disabled="busy || !newTitle.trim()">
           {{ t("shelf.create_and_write") }}
         </button>
      </form>

      <p v-if="note" class="shelf__note dialog__note">{{ note }}</p>
      <p v-if="entries.length === 0" class="shelf__empty dialog__empty">{{ t("shelf.empty") }}</p>
      <ul ref="listEl" class="shelf__list">
        <li
          v-for="entry in entries"
          :key="entry.id"
          class="shelf__card"
          :class="{ 'shelf__card--current': entry.id === workId }"
        >
          <div class="shelf__main" @dblclick="startRename(entry.id, entry.title)">
            <input
              v-if="renaming === entry.id"
              v-model="draft"
              class="shelf__rename"
              type="text"
              @click.stop
              @keydown.enter="commitRename(entry.id)"
              @keydown.esc="renaming = null"
              @blur="commitRename(entry.id)"
            />
            <span v-else class="shelf__name" :title="workLabel(entry.title)">{{ workLabel(entry.title) }}</span>
            <span class="shelf__meta">
              {{ shelfKindLabel(entry.kind) }} · {{ shelfLabel(entry, caliber) }} ·
              {{ formatWhen(entry.opened_at) }}
            </span>
            <!-- 简介：存着就显示一行（点「简介」改），没写就不占地方 -->
            <span
              v-if="entry.summary && noting !== entry.id"
              class="shelf__summary-line"
              :title="entry.summary"
            >
              {{ entry.summary }}
            </span>
            <textarea
              v-if="noting === entry.id"
              v-model="noteDraft"
              class="shelf__summary"
              rows="3"
              :placeholder="t('shelf.summary_placeholder')"
              @keydown.esc="noting = null"
            ></textarea>
            <span v-if="noting === entry.id" class="shelf__summary-actions">
              <button
                type="button"
                class="shelf__button dialog__button"
                :disabled="busy"
                @click="commitNote(entry.id)"
              >
                {{ t("common.save") }}
              </button>
              <button
                type="button"
                class="shelf__button dialog__button"
                :disabled="busy"
                @click="noting = null"
              >
                {{ t("common.cancel") }}
              </button>
            </span>
          </div>

          <div class="shelf__actions">
            <span v-if="entry.id === workId" class="shelf__here">{{ t("shelf.writing_now") }}</span>
            <button
              v-else
              type="button"
              class="shelf__button dialog__button"
              :disabled="busy"
              @click="open(entry.id)"
            >
              {{ t("shelf.open") }}
            </button>
            <button
              type="button"
              class="shelf__button dialog__button"
              :disabled="busy"
              :title="t('shelf.export_title')"
              @click="exportWork(entry.id, 'both')"
            >
              {{ t("shelf.export") }}
            </button>
            <button
              type="button"
              class="shelf__button dialog__button"
              :disabled="busy"
              :title="t('shelf.summary_title')"
              @click="startNote(entry.id, entry.summary)"
            >
              {{ t("shelf.summary_button") }}
            </button>
            <button
              type="button"
              class="shelf__button dialog__button"
              :disabled="busy"
              @click="startRename(entry.id, entry.title)"
            >
              {{ t("shelf.rename") }}
            </button>
            <button
              type="button"
              class="shelf__button shelf__button--danger dialog__button"
              :disabled="busy"
              @click="confirmRemove(entry.id, entry.title)"
            >
              {{ t("shelf.delete") }}
            </button>
          </div>
        </li>
      </ul>
    </section>
  </div>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./shelf-dialog.css"></style>
