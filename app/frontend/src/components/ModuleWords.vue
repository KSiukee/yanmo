<script setup lang="ts">
// 词表编辑：作者在软件里维护"有哪些名字、各是什么"，模块照着它在正文里找候选。
import { onMounted, ref } from "vue";

import { moduleWords, moduleWordsWrite, type WordRow } from "../api/modules";
import { t } from "../locales/index.ts";

const props = defineProps<{ moduleId: string; busy: boolean }>();
const emit = defineEmits<{ done: [message: string] }>();

const rows = ref<WordRow[]>([]);
const open = ref(false);
const loading = ref(false);

async function load() {
  loading.value = true;
  try {
    rows.value = (await moduleWords(props.moduleId)).items;
  } catch {
    rows.value = [];
  }
  loading.value = false;
}

function add() {
  rows.value.push({ name: "", kind: "setting" });
}

function drop(index: number) {
  rows.value.splice(index, 1);
}

async function save() {
  try {
    const count = await moduleWordsWrite(props.moduleId, rows.value);
    emit("done", t("modules.words_saved", { count }));
    await load();
  } catch (error) {
    emit("done", t("modules.action_failed", { detail: String(error) }));
  }
}

onMounted(() => void load());
</script>

<template>
  <section class="modules__candidates">
    <p class="modules__candidate">
      <span class="modules__candidate-text">{{ t("modules.words_title", { count: rows.length }) }}</span>
      <button type="button" class="modules__button dialog__button" @click="open = !open">
        {{ open ? t("modules.words_close") : t("modules.words_edit") }}
      </button>
    </p>
    <template v-if="open">
      <p v-for="(row, index) in rows" :key="index" class="modules__candidate">
        <input
          v-model="row.name"
          class="modules__input dialog__input"
          :placeholder="t('modules.words_name')"
        />
        <select v-model="row.kind" class="modules__select dialog__select">
          <option value="setting">{{ t("modules.words_kind_setting") }}</option>
          <option value="person">{{ t("modules.words_kind_person") }}</option>
        </select>
        <button type="button" class="modules__button dialog__button" @click="drop(index)">
          {{ t("modules.words_drop") }}
        </button>
      </p>
      <p class="modules__candidate">
        <button type="button" class="modules__button dialog__button" @click="add">
          {{ t("modules.words_add") }}
        </button>
        <button
          type="button"
          class="modules__button dialog__button"
          :disabled="busy || loading"
          @click="save"
        >
          {{ t("modules.words_save") }}
        </button>
      </p>
    </template>
  </section>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./module-dialog.css"></style>
