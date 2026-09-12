<script setup lang="ts">
// 版本历史：**这一章丢过什么，在这里都找得回来**。
//
// 每一行是核心留下的一版：点一下看它和现在差在哪；回滚会覆盖正文，所以先问一句——
// 而且核心在覆盖前还会替现在这一版留个底（回滚错了能再换回来）。
import type { EditorSession } from "../editor/session";
import type { SnapshotSummary } from "../api/core";
import { snapshotReason } from "../editor/snapshots";
import { formatWhen } from "../editor/display";
import { t } from "../locales/index.ts";

const props = defineProps<{ session: EditorSession }>();
const { entries, busy, note, selected, diff, close, compare, keep, drop, restore } =
  props.session.snapshots;

async function doRestore(entry: SnapshotSummary) {
  if (!window.confirm(t("snapshots.restore_confirm"))) return;
  await restore(entry);
}

async function doDrop(entry: SnapshotSummary) {
  if (!window.confirm(t("snapshots.drop_confirm"))) return;
  await drop(entry);
}
</script>

<template>
  <div class="snapshots dialog" @click.self="close">
    <section class="snapshots__box dialog__box">
      <header class="snapshots__head dialog__head">
        <h2 class="snapshots__title dialog__title">{{ t("snapshots.title") }}</h2>
        <button
          type="button"
          class="snapshots__button dialog__button"
          :title="t('snapshots.keep_title')"
          :disabled="busy"
          @click="keep"
        >
          {{ t("snapshots.keep") }}
        </button>
        <button type="button" class="snapshots__button dialog__button" @click="close">
          {{ t("common.close") }}
        </button>
      </header>

      <p v-if="note" class="snapshots__note dialog__note">{{ note }}</p>
      <p v-if="entries.length === 0" class="snapshots__empty dialog__empty">
        {{ t("snapshots.empty") }}
      </p>

      <div v-else class="snapshots__body">
        <ul class="snapshots__list">
          <li
            v-for="entry in entries"
            :key="entry.id"
            class="snapshots__row"
            :class="{ 'snapshots__row--on': entry.id === selected }"
          >
            <button
              type="button"
              class="snapshots__pick"
              :title="t('snapshots.compare_title')"
              @click="compare(entry)"
            >
              <span class="snapshots__when">{{ formatWhen(entry.created_at) }}</span>
              <span class="snapshots__meta">
                <span v-if="entry.pinned" class="snapshots__pin">{{ t("snapshots.pinned") }}</span>
                <span>{{ snapshotReason(entry.reason) }}</span>
                <span>{{ t("snapshots.words", { count: entry.char_count }) }}</span>
              </span>
            </button>
            <span class="snapshots__actions">
              <button
                type="button"
                class="snapshots__button dialog__button"
                :title="t('snapshots.restore_title')"
                :disabled="busy"
                @click="doRestore(entry)"
              >
                {{ t("snapshots.restore") }}
              </button>
              <button
                type="button"
                class="snapshots__button snapshots__button--danger dialog__button"
                :title="t('snapshots.drop_title')"
                :disabled="busy"
                @click="doDrop(entry)"
              >
                {{ t("snapshots.drop") }}
              </button>
            </span>
          </li>
        </ul>

        <section class="snapshots__diff">
          <header class="snapshots__diff-head">
            <span class="snapshots__diff-title">{{ t("snapshots.diff_title") }}</span>
            <span v-if="diff" class="snapshots__diff-stats">
              {{ t("snapshots.diff_stats", { added: diff.added, removed: diff.removed }) }}
            </span>
          </header>
          <p v-if="!diff" class="snapshots__diff-hint">{{ t("snapshots.compare_title") }}</p>
          <p v-else-if="diff.lines.length === 0" class="snapshots__diff-hint">
            {{ t("snapshots.diff_same") }}
          </p>
          <template v-else>
            <p v-if="diff.truncated" class="snapshots__diff-hint">
              {{ t("snapshots.diff_truncated") }}
            </p>
            <ol class="snapshots__lines">
              <li
                v-for="(line, index) in diff.lines"
                :key="index"
                class="snapshots__line"
                :class="`snapshots__line--${line.kind}`"
              >
                <span v-if="line.kind === 'skipped'" class="snapshots__gap">
                  {{ t("snapshots.diff_skipped", { count: line.hidden }) }}
                </span>
                <template v-else>
                  <span class="snapshots__sign">
                    {{ line.kind === "added" ? "+" : line.kind === "removed" ? "−" : " " }}
                  </span>
                  <span class="snapshots__text">{{ line.text }}</span>
                </template>
              </li>
            </ol>
          </template>
        </section>
      </div>
    </section>
  </div>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./snapshot-dialog.css"></style>
