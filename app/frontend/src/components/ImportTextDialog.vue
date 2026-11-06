<script setup lang="ts">
// 「导入外部文本」：粘贴一段，或选一个 txt / md → 碎片池。
//
// 三条分寸：
// - **只放不改**：导入只往碎片池里放，正文一个字都不动；
// - **先看再动**：分批方式与条数在导入前看清楚，重复的条目会给"跳过几条"；
// - **一批是一笔账**：导完可以整批撤，历史里也列得出来。
import { computed, onMounted, ref, watch } from "vue";

import type { EditorSession } from "../editor/session.ts";
import type { ImportBatch } from "../api/core";
import {
  fragmentImport,
  fragmentImportBatches,
  fragmentImportPreview,
  fragmentImportUndo,
  pickTextFile,
} from "../api/core";
import { t } from "../locales/index.ts";
import { formatWhen } from "../editor/display.ts";

const props = defineProps<{ session: EditorSession }>();
const emit = defineEmits<{ close: [] }>();

const text = ref("");
const label = ref("");
const mode = ref<"blank" | "line" | "whole">("blank");
const kind = ref<"idea" | "event">("idea");
const busy = ref(false);
const note = ref("");
const failure = ref("");
const preview = ref<{ count: number; total_chars: number; samples: string[] } | null>(null);
const batches = ref<ImportBatch[]>([]);
const lastBatch = ref("");

const workId = computed(() => props.session.workId.value);

async function refreshBatches() {
  if (workId.value === null) return;
  try {
    batches.value = await fragmentImportBatches(workId.value);
  } catch {
    // 读不到历史不影响导入本身
  }
}

let timer: ReturnType<typeof setTimeout> | null = null;
watch([text, mode], () => {
  if (timer) clearTimeout(timer);
  preview.value = null;
  if (!text.value.trim()) return;
  timer = setTimeout(async () => {
    try {
      preview.value = await fragmentImportPreview(text.value, mode.value);
    } catch {
      preview.value = null;
    }
  }, 250);
});

async function chooseFile() {
  failure.value = "";
  try {
    const file = await pickTextFile(t("import.pick_title"), t("import.pick_filter"));
    if (!file) return;
    text.value = file.text;
    label.value = file.name;
  } catch (error) {
    failure.value = error instanceof Error ? error.message : String(error);
  }
}

async function run() {
  if (busy.value || workId.value === null) return;
  failure.value = "";
  note.value = "";
  busy.value = true;
  try {
    const outcome = await fragmentImport(
      workId.value,
      kind.value,
      label.value.trim() || t("import.label_unnamed"),
      text.value,
      mode.value,
    );
    lastBatch.value = outcome.batch_id;
    note.value = t("import.done", {
      imported: outcome.imported,
      skipped: outcome.skipped,
      total: outcome.total,
    });
    if (outcome.imported > 0) {
      text.value = "";
      await props.session.creator.refresh();
    }
    await refreshBatches();
  } catch (error) {
    failure.value = error instanceof Error ? error.message : String(error);
  } finally {
    busy.value = false;
  }
}

async function undo(batch: string) {
  failure.value = "";
  try {
    const removed = await fragmentImportUndo(batch);
    note.value = t("import.undone", { count: removed });
    if (batch === lastBatch.value) lastBatch.value = "";
    await props.session.creator.refresh();
    await refreshBatches();
  } catch (error) {
    failure.value = error instanceof Error ? error.message : String(error);
  }
}

onMounted(() => void refreshBatches());
</script>

<template>
  <div class="import dialog" @click.self="emit('close')">
    <section class="import__box dialog__box">
      <header class="dialog__head">
        <h2 class="dialog__title">{{ t("import.title") }}</h2>
        <button type="button" class="dialog__close" @click="emit('close')">✕</button>
      </header>

      <p class="dialog__note">{{ t("import.lead") }}</p>
      <p v-if="failure" class="error">{{ failure }}</p>

      <div class="import__row">
        <button type="button" class="dialog__button" @click="void chooseFile()">
          {{ t("import.pick") }}
        </button>
        <span class="import__hint">{{ t("import.pick_hint") }}</span>
      </div>

      <label class="import__field">
        <span>{{ t("import.label") }}</span>
        <input v-model="label" type="text" :placeholder="t('import.label_placeholder')" />
      </label>

      <label class="import__field">
        <span>{{ t("import.mode") }}</span>
        <select v-model="mode">
          <option value="blank">{{ t("import.mode_blank") }}</option>
          <option value="line">{{ t("import.mode_line") }}</option>
          <option value="whole">{{ t("import.mode_whole") }}</option>
        </select>
      </label>

      <label class="import__field">
        <span>{{ t("import.kind") }}</span>
        <select v-model="kind">
          <option value="idea">{{ t("import.kind_idea") }}</option>
          <option value="event">{{ t("import.kind_event") }}</option>
        </select>
      </label>

      <label class="import__field">
        <span>{{ t("import.text") }}</span>
        <textarea v-model="text" rows="7" :placeholder="t('import.text_placeholder')"></textarea>
      </label>

      <p v-if="preview" class="import__preview">
        {{ t("import.preview", { count: preview.count, chars: preview.total_chars }) }}
      </p>
      <ul v-if="preview" class="import__samples">
        <li v-for="(sample, index) in preview.samples" :key="index">{{ sample }}</li>
      </ul>

      <div class="import__actions">
        <button
          type="button"
          class="dialog__button dialog__button--primary"
          :disabled="busy || !text.trim() || workId === null"
          @click="void run()"
        >
          {{ busy ? t("import.running") : t("import.run") }}
        </button>
        <button
          v-if="lastBatch"
          type="button"
          class="dialog__button"
          @click="void undo(lastBatch)"
        >
          {{ t("import.undo_last") }}
        </button>
        <span v-if="note" class="import__hint">{{ note }}</span>
      </div>

      <template v-if="batches.length > 0">
        <h3 class="import__history">{{ t("import.history") }}</h3>
        <ul class="import__batches">
          <li v-for="item in batches" :key="item.id" class="import__batch">
            <span class="import__batch-main">
              {{ item.label }} · {{ t("import.batch_line", { count: item.count, when: formatWhen(item.created_at) }) }}
            </span>
            <button
              type="button"
              class="dialog__button"
              :disabled="item.undone"
              @click="void undo(item.id)"
            >
              {{ item.undone ? t("import.batch_undone") : t("import.batch_undo") }}
            </button>
          </li>
        </ul>
      </template>
    </section>
  </div>
</template>

<style scoped src="./dialog.css"></style>

<style scoped>
.import__box {
  max-width: 640px;
}

.import__row {
  display: flex;
  gap: 8px;
  align-items: center;
  margin: 8px 0;
}

.import__field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin: 8px 0;
  font-size: 12px;
}

.import__field input,
.import__field select,
.import__field textarea {
  width: 100%;
}

.import__hint {
  font-size: 12px;
  opacity: 0.75;
}

.import__preview {
  font-size: 12px;
  margin: 6px 0 2px;
}

.import__samples {
  margin: 0 0 8px;
  padding-left: 18px;
  font-size: 12px;
  opacity: 0.8;
}

.import__actions {
  display: flex;
  gap: 8px;
  align-items: center;
  flex-wrap: wrap;
  margin-top: 10px;
}

.import__history {
  margin: 16px 0 6px;
  font-size: 13px;
}

.import__batches {
  list-style: none;
  margin: 0;
  padding: 0;
}

.import__batch {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  align-items: center;
  padding: 4px 0;
  font-size: 12px;
}

.import__batch-main {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
