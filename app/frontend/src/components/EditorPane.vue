<script setup lang="ts">
// 正文编辑器面板：**只挂当前章**（边写边存 + 切章 + 关窗闸门都在 editor/session.ts 里）。
//
// 这个文件只负责"长什么样"：会话编排是另一个变化理由，别混在一起。
// 会话由布局层建一次（目录树与编辑器说的是同一本书、同一章），这里只把它摊开。
import { computed } from "vue";
import { EditorContent } from "@tiptap/vue-3";

import { t } from "../locales/index.ts";
import {
  caliberLabelKey,
  countUnitKey,
  languageLabelKey,
  pickCount,
} from "../editor/wordcount.ts";
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
