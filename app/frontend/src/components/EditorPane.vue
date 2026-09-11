<script setup lang="ts">
// 正文编辑器面板：**只挂当前章**（边写边存 + 切章 + 关窗闸门都在 editor/session.ts 里）。
//
// 这个文件只负责"长什么样"：会话编排是另一个变化理由，别混在一起。
// 会话由布局层建一次（目录树与编辑器说的是同一本书、同一章），这里只把它摊开。
import { EditorContent } from "@tiptap/vue-3";

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

const STATUS_TEXT: Record<AutosaveState["status"], string> = {
  idle: "尚未落盘",
  pending: "待落盘…",
  saving: "保存中…",
  saved: "已保存",
  error: "保存异常",
  desync: "已抢救",
};
</script>

<template>
  <section class="editor">
    <header class="editor__bar">
      <span class="editor__title">{{ chapterTitle || "正文" }}</span>
      <span class="editor__meta">
        <span v-if="failure" class="editor__bad">{{ failure }}</span>
        <template v-else>
          <span class="editor__count">{{ saveState.word_count }} 字</span>
          <span
            class="editor__status"
            :class="`editor__status--${saveState.status}`"
            :title="saveState.detail"
          >
            {{ STATUS_TEXT[saveState.status] }}
          </span>
        </template>
      </span>
    </header>

    <p v-if="crashNotice" class="editor__notice">⚠ {{ crashNotice }}</p>
    <p v-if="saveState.incident" class="editor__notice">
      ⚠ 刚才有一次{{ saveState.incident }}——已自动救回并留下快照，建议回头核对一眼。
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
