<script setup lang="ts">
// 正文编辑器面板：**只挂当前章**（边写边存 + 切章 + 关窗闸门都在 editor/session.ts 里）。
//
// 这个文件只负责"长什么样"：会话编排是另一个变化理由，别混在一起。
// 会话由布局层建一次（目录树与编辑器说的是同一本书、同一章），这里只把它摊开。
import { computed, nextTick, ref } from "vue";
import { EditorContent } from "@tiptap/vue-3";

import { t } from "../locales/index.ts";
import {
  caliberLabelKey,
  countUnitKey,
  languageLabelKey,
  pickCount,
} from "../editor/wordcount.ts";
import { goalProgress, goalReached, pickDayCount } from "../editor/writing-days.ts";
import type { EditorSession } from "../editor/session";
import type { AutosaveState } from "../editor/autosave";
import ExitDialog from "./ExitDialog.vue";

const props = defineProps<{ session: EditorSession }>();
// 摊开之后每一项都是顶层绑定，模板里照常自动解包
const {
  editor,
  chapterTitle,
  saveState,
  exitState,
  failure,
  crashNotice,
  retryExit,
  escapeExit,
  forceExit,
  language,
  caliber,
  cycleCaliber,
  cycleLanguage,
} = props.session;

// 今日进度：账本在核心（`writing_days`），会话在每次落盘成功后刷一次。
// 没设目标就只显示"今天写了多少"——不拿一个假目标糊弄作者。
const {
  today: todayWriting,
  goal: dailyGoal,
  open: openWriting,
} = props.session.writing;
const todayCount = computed(() => pickDayCount(todayWriting.value, caliber.value));
const todayProgress = computed(() => goalProgress(todayCount.value, dailyGoal.value));
const todayReached = computed(() => goalReached(todayCount.value, dailyGoal.value));
const todayText = computed(() =>
  dailyGoal.value === null
    ? t("editor.today", { count: todayCount.value })
    : t("editor.today_of_goal", { count: todayCount.value, goal: dailyGoal.value }),
);

// 状态栏那个数字：三个口径都在手上（落盘时一起回来），**点一下就换一个**。
// 口径名与单位都从字典取（核心只给码，文案在 locales 里）。
const countText = computed(() =>
  t(countUnitKey(caliber.value), { count: pickCount(saveState.value, caliber.value) }),
);
const countTitle = computed(() =>
  t("editor.caliber.switch_title", { name: t(caliberLabelKey(caliber.value)) }),
);
const languageText = computed(() => t(languageLabelKey(language.value)));
const languageTitle = computed(() =>
  t("editor.language.switch_title", { name: languageText.value }),
);
// 版本历史：跟当前章绑在一起，入口就在章名这一行（状态机在 editor/snapshots.ts）
const { toggle: toggleSnapshots } = props.session.snapshots;
// 排版清理：同一条思路——一次动作、只作用于当前章（状态机在 editor/typeset.ts）
const { open: openTypeset } = props.session.typeset;
// 这一章的"一句话"（投稿包的大纲要用它）：**单独存、单独显示**，不跟正文搅在一起
const {
  value: noteValue,
  draft: noteDraft,
  editing: noteEditing,
  busy: noteBusy,
  open: openNote,
  save: saveNote,
  cancel: cancelNote,
} = props.session.note;
const noteEl = ref<HTMLInputElement | null>(null);

/** 点开"一句话"：打开输入框并聚焦（像目录树改名那样，点开就写，不打断写作） */
async function startNote() {
  openNote();
  await nextTick();
  noteEl.value?.focus();
}

// 状态 → 字典键：文案在 locales 里，这里只留"哪个状态对应哪句话"
const STATUS_KEYS: Record<AutosaveState["status"], string> = {
  idle: "editor.status.idle",
  pending: "editor.status.pending",
  saving: "editor.status.saving",
  saved: "editor.status.saved",
  error: "editor.status.error",
  desync: "editor.status.desync",
};

function statusText(status: AutosaveState["status"]): string {
  return t(STATUS_KEYS[status]);
}
</script>

<template>
  <section class="editor">
    <header class="editor__bar">
      <span class="editor__title">{{ chapterTitle || t("editor.untitled") }}</span>
      <button
        type="button"
        class="editor__versions"
        :title="t('snapshots.open_title')"
        @click="toggleSnapshots()"
      >
        {{ t("snapshots.button") }}
      </button>
      <button
        type="button"
        class="editor__versions"
        :title="t('typeset.button_title')"
        @click="void openTypeset()"
      >
        {{ t("typeset.button") }}
      </button>
      <button
        type="button"
        class="editor__versions"
        :title="t('editor.note_title')"
        @click="void startNote()"
      >
        {{ t("editor.note_button") }}
      </button>
      <span class="editor__meta">
        <span v-if="failure" class="editor__bad" :title="failure">{{ failure }}</span>
        <template v-else>
          <button
            type="button"
            class="editor__count"
            :title="countTitle"
            @click="cycleCaliber()"
          >
            {{ countText }}
          </button>
          <button
            type="button"
            class="editor__lang"
            :title="languageTitle"
            @click="cycleLanguage()"
          >
            {{ languageText }}
          </button>
          <!-- 今日进度：点一下开码字日历（进度条只在设过目标时画）。宽度钉死，
               免得好字与数字变长时把整条栏推着晃（见 editor-pane.css 的说明） -->
          <button
            type="button"
            class="editor__today"
            :class="{ 'editor__today--goal': todayProgress !== null }"
            :title="t('editor.today_title')"
            @click="void openWriting()"
          >
            <span class="editor__today-num" :class="{ 'editor__today-num--done': todayReached }">
              {{ todayText }}
            </span>
            <span v-if="todayProgress !== null" class="editor__today-bar">
              <span
                class="editor__today-fill"
                :style="{ width: `${Math.round(todayProgress * 100)}%` }"
              />
            </span>
          </button>
          <span
            class="editor__status"
            :class="`editor__status--${saveState.status}`"
            :title="saveState.detail"
          >
            {{ statusText(saveState.status) }}
          </span>
        </template>
      </span>
    </header>

    <p v-if="crashNotice" class="editor__notice">⚠ {{ crashNotice }}</p>
    <p v-if="saveState.incident" class="editor__notice">
      {{ t("editor.incident", { kind: saveState.incident }) }}
    </p>

    <!-- 这一章的"一句话"：写入框就在正文上方，存了就显示一行，点它接着改 -->
    <form v-if="noteEditing" class="editor__note" @submit.prevent="void saveNote()">
      <input
        ref="noteEl"
        v-model="noteDraft"
        class="editor__note-input"
        type="text"
        :placeholder="t('editor.note_placeholder')"
        @keydown.esc="cancelNote()"
      />
      <button type="submit" class="editor__note-button" :disabled="noteBusy">
        {{ t("common.save") }}
      </button>
      <button type="button" class="editor__note-button" @click="cancelNote()">
        {{ t("common.cancel") }}
      </button>
    </form>
    <p
      v-else-if="noteValue"
      class="editor__note-line"
      :title="t('editor.note_title')"
      @click="void startNote()"
    >
      <span class="editor__note-label">{{ t("editor.note_label") }}</span>{{ noteValue }}
    </p>

    <EditorContent v-if="editor" :editor="editor" class="editor__area" />

    <ExitDialog
      v-if="exitState.blocked"
      :message="exitState.message"
      :escape-path="exitState.escapePath"
      :busy="exitState.busy"
      @retry="retryExit"
      @escape="escapeExit"
      @force="forceExit"
    />
  </section>
</template>

<style scoped src="./editor-pane.css"></style>
