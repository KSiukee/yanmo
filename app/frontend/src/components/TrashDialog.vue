<script setup lang="ts">
// 回收站：**删错了要能捞回来**——所以这里每一行都给"恢复"，而"彻底删除"必须先问一句。
import { ref } from "vue";

import type { EditorSession } from "../editor/session";
import { trashLabel } from "../editor/trash";
import { t } from "../locales/index.ts";
import { formatCaliberWords, formatWhen } from "../editor/display";

const props = defineProps<{ session: EditorSession }>();
const { entries, busy, close, restore, purge, empty, conflict, resolveConflict, cancelConflict } =
  props.session.trash;
// 重名提示里那个「已写多少字」也按当前口径念（跟状态栏、目录树同一口径）
const { caliber } = props.session;
/** 冲突时作者填的新名字（留空 = 照原样恢复，两章同名） */
const renameTo = ref("");

/** 名字可能为空（没起名的那一本 / 一个卷）：显示的永远是"有话说"的名字 */
function nameOf(title: string): string {
  return title || t("common.untitled");
}

/** 上一次动作的交代（"捞回来了""彻底删了 N 项"） */
const note = ref("");

async function doRestore(entry: Parameters<typeof restore>[0]) {
  note.value = await restore(entry);
}

async function doPurge(entry: Parameters<typeof purge>[0]) {
  const what = trashLabel(entry);
  if (!window.confirm(t("trash.purge_confirm", { what }))) return;
  note.value = await purge(entry);
}

async function doResolve(rename: boolean) {
  await resolveConflict(rename && renameTo.value.trim() ? renameTo.value.trim() : null);
  renameTo.value = "";
  note.value = t(rename ? "trash.restored_renamed" : "trash.restored_as_is");
}

async function doEmpty() {
  const question = t("trash.empty_confirm", { count: entries.value.length });
  if (!window.confirm(question)) return;
  await empty();
  note.value = t("trash.emptied");
}
</script>

<template>
  <div class="trash dialog" @click.self="close">
    <section class="trash__box dialog__box">
      <header class="trash__head dialog__head">
        <h2 class="trash__title dialog__title">{{ t("trash.title") }}</h2>
        <button
          type="button"
          class="trash__button trash__button--danger dialog__button"
          :disabled="busy || entries.length === 0"
          @click="doEmpty"
        >
          {{ t("trash.empty") }}
        </button>
        <button type="button" class="trash__button dialog__button" :title="t('trash.close_title')" @click="close">{{ t("common.close") }}</button>
      </header>

      <p v-if="note" class="trash__note dialog__note">{{ note }}</p>

      <div v-if="conflict" class="trash__conflict">
        <p class="trash__conflict-title">
          {{ t("trash.conflict_title", { title: nameOf(conflict.entry.title) }) }}
        </p>
        <ul class="trash__clashes">
          <li v-for="clash in conflict.preview.name_clashes" :key="clash.id">
            {{ t("trash.clash_line", { title: clash.title, words: formatCaliberWords(clash, caliber) }) }}
          </li>
        </ul>
        <p class="trash__conflict-hint">
          {{ t("trash.conflict_hint") }}
        </p>
        <div class="trash__conflict-actions">
          <input v-model="renameTo" class="trash__input dialog__input" type="text" :placeholder="t('trash.rename_placeholder')" />
          <button type="button" class="trash__button dialog__button" :disabled="busy" @click="doResolve(true)">
            {{ t("trash.restore") }}
          </button>
          <button type="button" class="trash__button dialog__button" :disabled="busy" @click="cancelConflict">
            {{ t("trash.cancel") }}
          </button>
        </div>
      </div>
      <p v-if="entries.length === 0" class="trash__empty dialog__empty">{{ t("trash.empty_list") }}</p>

      <ul class="trash__list">
        <li v-for="entry in entries" :key="`${entry.kind}-${entry.id}`" class="trash__row">
          <div class="trash__main">
            <span class="trash__name">{{ trashLabel(entry) }}</span>
            <span class="trash__meta">{{ t("trash.deleted_at", { when: formatWhen(entry.deleted_at) }) }}</span>
          </div>
          <div class="trash__actions">
            <button
              type="button"
              class="trash__button dialog__button"
              :disabled="busy"
              :title="t('trash.restore_title')"
              @click="doRestore(entry)"
            >
              {{ t("trash.restore") }}
            </button>
            <button
              type="button"
              class="trash__button trash__button--danger dialog__button"
              :disabled="busy"
              @click="doPurge(entry)"
            >
              {{ t("trash.purge") }}
            </button>
          </div>
        </li>
      </ul>
    </section>
  </div>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./trash-dialog.css"></style>
