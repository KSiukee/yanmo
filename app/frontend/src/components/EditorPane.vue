<script setup lang="ts">
// 正文编辑器面板：**只挂当前章**（边写边存 + 切章 + 关窗闸门都在 editor/session.ts 里）。
//
// 这个文件只负责"长什么样"：会话编排是另一个变化理由，别混在一起。
// 会话由布局层建一次（目录树与编辑器说的是同一本书、同一章），这里只把它摊开。
import { EditorContent } from "@tiptap/vue-3";

import { t } from "../locales/index.ts";
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
} = props.session;

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
      <span class="editor__meta">
        <span v-if="failure" class="editor__bad" :title="failure">{{ failure }}</span>
        <template v-else>
          <span class="editor__count">{{ t("editor.word_count", { count: saveState.word_count }) }}</span>
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
