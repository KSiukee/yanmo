<script setup lang="ts">
// 书架：**多作品是默认形态**——这一屏就是"我手上有哪几本书、各写到什么程度"。
//
// 只在打开时拉一次列表（书架不是常驻画面），切书交给会话层（先落盘再切）。
import { nextTick, ref } from "vue";

import type { EditorSession } from "../editor/session";
import { shelfKindLabel, shelfLabel } from "../editor/shelf";
import { formatWhen } from "../editor/display";

const props = defineProps<{ session: EditorSession }>();
const { entries, busy, close, open, create, rename, remove, export: exportWork, note } =
  props.session.shelf;
const { visible: trashVisible, toggle: toggleTrash } = props.session.trash;
const { workId } = props.session;

/** 去回收站：先把书架收起来，免得两层弹层叠在一起 */
function openTrash() {
  close();
  toggleTrash();
}

/** 正在改名的那一本（同时只可能有一本） */
const renaming = ref<number | null>(null);
const draft = ref("");
const listEl = ref<HTMLElement | null>(null);

/** 新建那本的表单 */
const creating = ref(false);
const newTitle = ref("");
const newKind = ref("novel");
const newTitleEl = ref<HTMLInputElement | null>(null);

const KINDS = [
  { value: "novel", label: "长篇（卷 → 章）" },
  { value: "collection", label: "短篇集（一篇一篇）" },
  { value: "article", label: "单篇（零层级）" },
];

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
  if (window.confirm(`删掉《${title}》？稿子会留在回收站里，之后可以捞回来。`)) {
    void remove(work_id);
  }
}
</script>

<template>
  <div class="shelf dialog" @click.self="close">
    <section class="shelf__box dialog__box">
      <header class="shelf__head dialog__head">
        <h2 class="shelf__title dialog__title">书架</h2>
        <button type="button" class="shelf__button dialog__button" :disabled="busy" @click="startCreate">
          + 新书
        </button>
        <button type="button" class="shelf__button dialog__button" title="看看删掉的书与章节" @click="openTrash">
          回收站
        </button>
        <button type="button" class="shelf__button dialog__button" title="回到正文" @click="close">关闭</button>
      </header>

      <form v-if="creating" class="shelf__new" @submit.prevent="submitCreate">
        <input
          ref="newTitleEl"
          v-model="newTitle"
          class="shelf__input"
          type="text"
          placeholder="书名"
          @keydown.esc="creating = false"
        />
        <select v-model="newKind" class="shelf__kind">
          <option v-for="kind in KINDS" :key="kind.value" :value="kind.value">
            {{ kind.label }}
          </option>
        </select>
        <button type="submit" class="shelf__button dialog__button" :disabled="busy || !newTitle.trim()">建好就写</button>
      </form>

      <p v-if="note" class="shelf__note dialog__note">{{ note }}</p>
      <p v-if="entries.length === 0" class="shelf__empty dialog__empty">书架上还没有书</p>
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
            <span v-else class="shelf__name" :title="entry.title">{{ entry.title }}</span>
            <span class="shelf__meta">
              {{ shelfKindLabel(entry.kind) }} · {{ shelfLabel(entry) }} ·
              {{ formatWhen(entry.opened_at) }}
            </span>
          </div>

          <div class="shelf__actions">
            <span v-if="entry.id === workId" class="shelf__here">正在写</span>
            <button
              v-else
              type="button"
              class="shelf__button dialog__button"
              :disabled="busy"
              @click="open(entry.id)"
            >
              打开
            </button>
            <button
              type="button"
              class="shelf__button dialog__button"
              :disabled="busy"
              title="导出成文件（分章 txt + 单文件 json），同样的内容不会重复写"
              @click="exportWork(entry.id, 'both')"
            >
              导出
            </button>
            <button
              type="button"
              class="shelf__button dialog__button"
              :disabled="busy"
              @click="startRename(entry.id, entry.title)"
            >
              改名
            </button>
            <button
              type="button"
              class="shelf__button shelf__button--danger dialog__button"
              :disabled="busy"
              @click="confirmRemove(entry.id, entry.title)"
            >
              删除
            </button>
          </div>
        </li>
      </ul>
    </section>
  </div>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./shelf-dialog.css"></style>
