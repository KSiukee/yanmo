<script setup lang="ts">
// 集视图：**打开一个集时主区就是这一屏**（集没有正文，编辑器让位）。
//
// 它管三件事：这一集收了哪几篇（顺序可挪）、集目标到哪了、还没归集的散篇怎么加进来。
// 合成一本集子（导出）归编译那一族，这里只管清单与目标——**集是引用，不是容器**。
import { computed, ref, watch } from "vue";

import type { EditorSession } from "../editor/session";
import { pieceProgress, wordProgress } from "../editor/collection";
import { formatCaliberNumber, formatWhen, formatWords } from "../editor/display";
import { t } from "../locales/index.ts";

const props = defineProps<{ session: EditorSession }>();
const { view, loose, busy, note, add, remove, move, setTarget } = props.session.collection;
const { caliber } = props.session;

const targetPieces = ref("");
const targetWords = ref("");

/** 目标输入框跟着"哪一集"走（换集就重填；同一集里改完不该被回读覆盖掉正在打的字）。 */
watch(
  () => view.value?.id,
  () => {
    targetPieces.value = view.value?.target_pieces == null ? "" : String(view.value.target_pieces);
    targetWords.value = view.value?.target_words == null ? "" : String(view.value.target_words);
  },
  { immediate: true },
);

function saveTargets() {
  const pieces = targetPieces.value.trim() === "" ? null : Number(targetPieces.value);
  const words = targetWords.value.trim() === "" ? null : Number(targetWords.value);
  void setTarget(
    pieces !== null && Number.isFinite(pieces) ? pieces : null,
    words !== null && Number.isFinite(words) ? words : null,
  );
}

const piece = computed(() =>
  view.value ? pieceProgress(view.value) : { have: 0, want: null, done: false },
);
const words = computed(() =>
  view.value ? wordProgress(view.value, caliber.value) : { have: 0, want: null, done: false },
);

/** 顶部那行进度：设过目标就把目标也说出来（"6 篇 · 1.2 万字（目标：8 篇 / 8 万字）"）。 */
const progressText = computed(() => {
  const base = t("collection.progress", {
    pieces: piece.value.have,
    words: formatWords(words.value.have),
  });
  if (piece.value.want === null && words.value.want === null) return base;
  return t("collection.progress_target", {
    base,
    pieces: piece.value.want ?? "—",
    words: words.value.want === null ? "—" : formatWords(words.value.want),
  });
});

/** 加入清单：还没归集的散篇 */
const picked = ref<number | null>(null);

function addPicked() {
  const id = picked.value ?? loose.value[0]?.id ?? null;
  if (id !== null) {
    void add(id);
    picked.value = null;
  }
}

/** 这一篇的字数（按当前口径，与集汇总同一个数法） */
function wordsOf(entry: { word_count: number; char_count: number; chars_no_punct: number }): string {
  return formatCaliberNumber(entry, caliber.value);
}

/** 打开这一篇：走稿件台那条"打开"（同一个切作品纪律，先落盘再切） */
function openArticle(id: number) {
  void props.session.manuscript.openWork(id);
}
</script>

<template>
  <section class="collection">
    <header class="collection__head">
      <h2 class="collection__title">{{ view?.title || t("common.untitled") }}</h2>
      <!-- 目标：**参考线**，不是判据。填了就显示进度，没填就不显示——不替作者立规矩。 -->
      <span class="collection__field">
        <span>{{ t("collection.target_pieces") }}</span>
        <input v-model="targetPieces" type="number" min="0" class="collection__input" />
      </span>
      <span class="collection__field">
        <span>{{ t("collection.target_words") }}</span>
        <input v-model="targetWords" type="number" min="0" class="collection__input" />
      </span>
      <button type="button" class="collection__button" :disabled="busy" @click="saveTargets">
        {{ t("common.save") }}
      </button>
      <span class="collection__meta">{{ progressText }}</span>
    </header>

    <p v-if="note" class="collection__note">{{ note }}</p>

    <ol class="collection__list">
      <li v-for="(entry, index) in view?.entries ?? []" :key="entry.id" class="collection__row">
        <span class="collection__index">{{ index + 1 }}</span>
        <span class="collection__name" :title="entry.title">{{ entry.title || t("common.untitled") }}</span>
        <span class="collection__meta">
          {{ t(`manuscript.status.${entry.status}`) }} · {{ wordsOf(entry) }} ·
          {{ formatWhen(entry.updated_at) }}
        </span>
        <span class="collection__actions">
          <button
            type="button"
            class="collection__button"
            :disabled="busy || index === 0"
            :title="t('collection.move_up')"
            @click="move(entry.id, -1)"
          >
            ↑
          </button>
          <button
            type="button"
            class="collection__button"
            :disabled="busy || index === (view?.entries.length ?? 1) - 1"
            :title="t('collection.move_down')"
            @click="move(entry.id, 1)"
          >
            ↓
          </button>
          <button type="button" class="collection__button" :disabled="busy" @click="openArticle(entry.id)">
            {{ t("collection.open") }}
          </button>
          <button
            type="button"
            class="collection__button collection__button--danger"
            :disabled="busy"
            :title="t('collection.remove_title')"
            @click="remove(entry.id)"
          >
            {{ t("collection.remove") }}
          </button>
        </span>
      </li>
    </ol>

    <p v-if="(view?.entries.length ?? 0) === 0" class="collection__empty">{{ t("collection.empty") }}</p>

    <footer class="collection__add">
      <span class="collection__meta">{{ t("collection.add_hint") }}</span>
      <select v-if="loose.length > 0" v-model="picked" class="collection__input">
        <option v-for="entry in loose" :key="entry.id" :value="entry.id">
          {{ entry.title || t("common.untitled") }}（{{ wordsOf(entry) }}）
        </option>
      </select>
      <button type="button" class="collection__button" :disabled="busy || loose.length === 0" @click="addPicked">
        {{ t("collection.add") }}
      </button>
      <span v-if="loose.length === 0" class="collection__meta">{{ t("collection.no_loose") }}</span>
    </footer>
  </section>
</template>

<style scoped src="./collection-pane.css"></style>
