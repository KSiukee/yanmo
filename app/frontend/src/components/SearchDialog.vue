<script setup lang="ts">
// 正文检索：打几个字，看它写在哪几章里。
//
// 分工（与别处同一条口径）：
// - **检索语义全在核心**（≥3 字走索引、1~2 字回退全扫、已删除的章不进结果）——这里不重做一遍；
// - 核心给的片段**不带高亮标记**，标哪一处由 `highlight` 在界面这边算（大小写不敏感、只标第一处）；
// - 点一条跳过去走的是会话那条「先落盘再切」的老路（命中在别的书里就先切书，见 session 的 search）。
//
// 状态机在 `editor/search.ts`：这里只管摆出来与接线，不自己管防抖与作废。
import { nextTick, ref } from "vue";

import type { EditorSession } from "../editor/session";
import { highlight } from "../editor/search.ts";
import { t } from "../locales/index.ts";

const props = defineProps<{ session: EditorSession }>();
const { query, hits, busy, error, truncated, type, run, open, close } = props.session.search;

const inputEl = ref<HTMLInputElement | null>(null);
// 打开就聚焦到输入框：这个弹窗的用处就是打字（焦点策略见 session 里的 anyDialogOpen）
void nextTick(() => inputEl.value?.focus());

/** 输入框动一下：交给状态机排防抖（到点才发车） */
function onType(event: Event) {
  const target = event.target;
  if (target instanceof HTMLInputElement) type(target.value);
}

/** 名字可能为空（没起名的那一本 / 一章）：显示的永远是"有话说"的名字 */
function nameOf(title: string): string {
  return title || t("common.untitled");
}
</script>

<template>
  <div class="search dialog" @click.self="close">
    <section class="search__box dialog__box">
      <header class="search__head dialog__head">
        <h2 class="search__title dialog__title">{{ t("search.title") }}</h2>
        <button
          type="button"
          class="search__button dialog__button"
          :title="t('search.close_title')"
          @click="close"
        >
          {{ t("common.close") }}
        </button>
      </header>

      <input
        ref="inputEl"
        class="search__input dialog__input"
        type="text"
        :value="query"
        :placeholder="t('search.placeholder')"
        @input="onType"
        @keydown.enter.prevent="void run()"
        @keydown.esc="close"
      />
      <p class="search__hint dialog__note">{{ t("search.hint") }}</p>

      <!-- 一行一种状态，一次只说一句：错了就说错，别再摆一个空态 -->
      <p v-if="error" class="search__error dialog__note">{{ t("search.failed", { detail: error }) }}</p>
      <p v-else-if="!query.trim()" class="search__empty dialog__empty">{{ t("search.idle") }}</p>
      <p v-else-if="busy" class="search__empty dialog__empty">{{ t("search.busy") }}</p>
      <p v-else-if="hits.length === 0" class="search__empty dialog__empty">{{ t("search.empty") }}</p>
      <p v-if="truncated" class="search__note dialog__note">
        {{ t("search.truncated", { count: hits.length }) }}
      </p>

      <ul class="search__list">
        <li v-for="hit in hits" :key="hit.node_id" class="search__row">
          <button type="button" class="search__open" @click="void open(hit)">
            <span class="search__where">
              {{ nameOf(hit.work_title) }} ·
              <!-- 命中在标题里：标题自己也要标出来（否则作者看不出这一行为什么算命中） -->
              <span v-if="hit.matched_title" class="search__chapter">
                <span
                  v-for="(part, index) in highlight(hit.title, query)"
                  :key="index"
                  :class="{ 'search__mark': part.hit }"
                >{{ part.text }}</span>
              </span>
              <span v-else class="search__chapter">{{ nameOf(hit.title) }}</span>
            </span>
            <span v-if="hit.snippet" class="search__snippet">
              <span
                v-for="(part, index) in highlight(hit.snippet, query)"
                :key="index"
                :class="{ 'search__mark': part.hit }"
              >{{ part.text }}</span>
            </span>
          </button>
        </li>
      </ul>
    </section>
  </div>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./search-dialog.css"></style>
