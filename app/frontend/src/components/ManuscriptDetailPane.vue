<script setup lang="ts">
// 稿件台的**右栏**：选中那一篇的完稿目标、可点的"改到哪一态"、全部投稿记录。
//
// 从弹窗里拆出来（它自己是一整块、只跟"选中的那一篇"打交道）：
// 弹窗那一件只管看板与那张表单，长起来也不至于挤成上帝组件。
import { computed, ref, watch } from "vue";

import type { EditorSession } from "../editor/session";
import type { ManuscriptStatus, Submission } from "../api/manuscript";
import { msToDateInput, nextStatuses, statusKey } from "../editor/manuscript";
import { t } from "../locales/index.ts";

const props = defineProps<{ session: EditorSession }>();
const { detail, rows, busy, openEdit, move, removeSubmission, setTarget } = props.session.manuscript;

/** 选中的那一篇在最新一屏里长什么样（目标输入框要用它的当前值）。 */
const selectedRow = computed(() => rows.value.find((row) => row.id === detail.value?.work_id) ?? null);

const targetInput = ref("");

function closeDetail() {
  detail.value = null;
}

/** 存目标：空 = 清掉（回到"没设过"）。 */
function saveTarget() {
  const row = selectedRow.value;
  if (!row) return;
  const text = targetInput.value.trim();
  const words = text === "" ? null : Number(text);
  void setTarget(row.id, words !== null && Number.isFinite(words) ? words : null);
}

/** 改这一条记录（表单在弹窗那一件里，这里只把"改哪一条"报上去）。 */
function editRecord(record: Submission) {
  const row = selectedRow.value;
  if (row) openEdit(row.id, record);
}

/**
 * 「改到哪一态」的可点入口：拖拽是鼠标专属，这一排按钮让每一步都点得到
 * （键盘、触控板、以及自动化验收都靠它；合法性仍与核心迁移表同源）。
 */
const moves = computed(() => (detail.value ? nextStatuses(detail.value.status) : []));

function moveSelected(to: ManuscriptStatus) {
  const row = detail.value;
  if (row) void move(row.work_id, to);
}

function confirmRemove(record: Submission) {
  if (window.confirm(t("manuscript.delete_record_confirm", { venue: record.venue }))) {
    void removeSubmission(record.id);
  }
}

/** 目标输入框跟着"选中的那一篇"重填。 */
function fillTarget(): void {
  const row = selectedRow.value;
  targetInput.value = row?.target_words == null ? "" : String(row.target_words);
}

// 选中哪一篇就重填哪一篇的目标（`detail` 换了才有必要动）
watch(() => detail.value?.work_id, fillTarget, { immediate: true });
</script>

<template>
  <aside v-if="detail && selectedRow" class="manuscript__side">
    <header class="manuscript__side-head">
      <span class="manuscript__name">{{ selectedRow.title || t("common.untitled") }}</span>
      <span class="manuscript__meta">{{ t(statusKey(detail.status)) }}</span>
      <button type="button" class="manuscript__button dialog__button" @click="closeDetail">
        {{ t("common.close") }}
      </button>
    </header>
    <p class="manuscript__field">
      <span>{{ t("manuscript.target_words") }}</span>
      <input v-model="targetInput" type="number" min="0" class="manuscript__input" />
      <button type="button" class="manuscript__button dialog__button" :disabled="busy" @click="saveTarget">
        {{ t("common.save") }}
      </button>
    </p>
    <p class="manuscript__hint">{{ t("manuscript.target_hint") }}</p>
    <p v-if="moves.length > 0" class="manuscript__moves">
      <span class="manuscript__hint">{{ t("manuscript.move_to") }}</span>
      <button
        v-for="to in moves"
        :key="to"
        type="button"
        class="manuscript__button dialog__button"
        :disabled="busy"
        @click="moveSelected(to)"
      >
        {{ t(statusKey(to)) }}
      </button>
    </p>
    <ul class="manuscript__history">
      <li v-for="record in detail.submissions" :key="record.id" class="manuscript__record">
        <span class="manuscript__meta">
          {{ t("manuscript.record_line", {
            venue: record.venue,
            at: msToDateInput(record.submitted_at),
            result: t(`manuscript.result.${record.result}`),
          }) }}
          <template v-if="record.cooldown_until !== null">
            · {{ t("manuscript.record_until", { until: msToDateInput(record.cooldown_until) }) }}
          </template>
        </span>
        <span v-if="record.notes" class="manuscript__notes">{{ record.notes }}</span>
        <span class="manuscript__actions">
          <button type="button" class="manuscript__button dialog__button" @click="editRecord(record)">
            {{ t("manuscript.edit_record") }}
          </button>
          <button
            type="button"
            class="manuscript__button manuscript__button--danger dialog__button"
            @click="confirmRemove(record)"
          >
            {{ t("manuscript.delete_record") }}
          </button>
        </span>
      </li>
      <li v-if="detail.submissions.length === 0" class="manuscript__hint">
        {{ t("manuscript.no_records") }}
      </li>
    </ul>
  </aside>
</template>

<style scoped src="./manuscript-dialog.css"></style>
