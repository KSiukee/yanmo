<script setup lang="ts">
// 回收站：**删错了要能捞回来**——所以这里每一行都给"恢复"，而"彻底删除"必须先问一句。
import { ref } from "vue";

import type { EditorSession } from "../editor/session";
import { trashLabel } from "../editor/trash";
import { formatWhen } from "../editor/display";

const props = defineProps<{ session: EditorSession }>();
const { entries, busy, close, restore, purge, empty, conflict, resolveConflict, cancelConflict } =
  props.session.trash;
/** 冲突时作者填的新名字（留空 = 照原样恢复，两章同名） */
const renameTo = ref("");

/** 上一次动作的交代（"捞回来了""彻底删了 N 项"） */
const note = ref("");

async function doRestore(entry: Parameters<typeof restore>[0]) {
  note.value = await restore(entry);
}

async function doPurge(entry: Parameters<typeof purge>[0]) {
  const what = trashLabel(entry);
  if (!window.confirm(`彻底删除 ${what}？这一步没有后悔药。`)) return;
  note.value = await purge(entry);
}

async function doResolve(rename: boolean) {
  await resolveConflict(rename && renameTo.value.trim() ? renameTo.value.trim() : null);
  renameTo.value = "";
  note.value = rename ? "已按你给的名字恢复" : "已照原样恢复（目录里会有两章同名）";
}

async function doEmpty() {
  if (!window.confirm(`清空回收站？里面的 ${entries.value.length} 项会被彻底删除，没有后悔药。`)) return;
  await empty();
  note.value = "回收站清空了";
}
</script>

<template>
  <div class="trash" @click.self="close">
    <section class="trash__box">
      <header class="trash__head">
        <h2 class="trash__title">回收站</h2>
        <button
          type="button"
          class="trash__button trash__button--danger"
          :disabled="busy || entries.length === 0"
          @click="doEmpty"
        >
          清空
        </button>
        <button type="button" class="trash__button" title="回到书架" @click="close">关闭</button>
      </header>

      <p v-if="note" class="trash__note">{{ note }}</p>

      <div v-if="conflict" class="trash__conflict">
        <p class="trash__conflict-title">
          要恢复的「{{ conflict.entry.title }}」跟还活着的一章**重名**了：
        </p>
        <ul class="trash__clashes">
          <li v-for="clash in conflict.preview.name_clashes" :key="clash.id">
            {{ clash.title }}（已写 {{ clash.word_count }} 字）
          </li>
        </ul>
        <p class="trash__conflict-hint">
          恢复**不会覆盖**你已经写的那一章——两章都会在。要给它换个名字就填在下面。
        </p>
        <div class="trash__conflict-actions">
          <input v-model="renameTo" class="trash__input" type="text" placeholder="给它起个新名字（留空 = 照原样恢复）" />
          <button type="button" class="trash__button" :disabled="busy" @click="doResolve(true)">
            恢复
          </button>
          <button type="button" class="trash__button" :disabled="busy" @click="cancelConflict">
            取消
          </button>
        </div>
      </div>
      <p v-if="entries.length === 0" class="trash__empty">回收站是空的</p>

      <ul class="trash__list">
        <li v-for="entry in entries" :key="`${entry.kind}-${entry.id}`" class="trash__row">
          <div class="trash__main">
            <span class="trash__name">{{ trashLabel(entry) }}</span>
            <span class="trash__meta">删于 {{ formatWhen(entry.deleted_at) }}</span>
          </div>
          <div class="trash__actions">
            <button
              type="button"
              class="trash__button"
              :disabled="busy"
              title="捞回原来待的地方"
              @click="doRestore(entry)"
            >
              恢复
            </button>
            <button
              type="button"
              class="trash__button trash__button--danger"
              :disabled="busy"
              @click="doPurge(entry)"
            >
              彻底删除
            </button>
          </div>
        </li>
      </ul>
    </section>
  </div>
</template>

<style scoped src="./trash-dialog.css"></style>
