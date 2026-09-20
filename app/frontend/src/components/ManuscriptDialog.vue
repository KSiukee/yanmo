<script setup lang="ts">
// 稿件台：**按状态看稿子**（书架按作品看，这里按流转看）。
//
// 与书架同源、同一批作品——不是第二份列表：卡片上的「打开」就是切到那一篇去写。
//
// 两条口径（与核心一致）：
// - 拖拽能落哪一列由 `canDrop` 判（与核心迁移表同源），拖到说不通的一列**当场说清为什么**；
// - 进「已投」不用拖的，用「记一次投稿」——那个状态的意思就是"投出去了、在等回音"，
//   不能空着手拖过去。
import { computed, ref } from "vue";

import type { EditorSession } from "../editor/session";
import type { ManuscriptRow, ManuscriptStatus, Submission } from "../api/manuscript";
import type { SubmitDraft } from "../editor/manuscript-board";
import {
  BOARD_COLUMNS,
  canDrop,
  daysUntil,
  groupByStatus,
  msToDateInput,
  nextStatuses,
  overdue,
  remainingWords,
  statusKey,
  unfinishedRows,
  untargetedRows,
} from "../editor/manuscript";
import { formatCaliberWords, formatWhen } from "../editor/display";
import { t } from "../locales/index.ts";
import ManuscriptRecordForm from "./ManuscriptRecordForm.vue";

const props = defineProps<{ session: EditorSession }>();
const {
  busy,
  note,
  rows,
  defaultCooldownDays,
  overdueCount,
  detail,
  form,
  warned,
  close,
  select,
  openSubmit,
  openEdit,
  closeForm,
  save,
  saveEdit,
  confirmWarned,
  move,
  removeSubmission,
  setTarget,
  openWork,
} = props.session.manuscript;
const { workId, caliber } = props.session;

/** 这一屏现在看哪一块：看板（按状态）/ 未完成散篇（开了坑没填的那种）。 */
const view = ref<"board" | "unfinished">("board");

const columns = computed(() => groupByStatus(rows.value));
const unfinished = computed(() => unfinishedRows(rows.value));
const untargeted = computed(() => untargetedRows(rows.value));
const selectedRow = computed(() => rows.value.find((row) => row.id === detail.value?.work_id) ?? null);

function workLabel(title: string): string {
  return title || t("common.untitled");
}

/** 卡片副标题：字数（按作品口径）+ 投过几家。 */
function cardMeta(row: ManuscriptRow): string {
  return t("manuscript.card_meta", {
    words: formatCaliberWords(row, caliber.value),
    count: row.submissions,
  });
}

/** 在等回音那一行：在等谁 + 还剩几天（超期了就直说）。 */
function activeLine(row: ManuscriptRow): string {
  if (row.active_venue === null) return "";
  if (overdue(row.active_cooldown_until)) {
    return t("manuscript.active_overdue", { venue: row.active_venue });
  }
  if (row.active_cooldown_until === null) {
    return t("manuscript.active_no_deadline", { venue: row.active_venue });
  }
  return t("manuscript.active_days", {
    venue: row.active_venue,
    days: daysUntil(row.active_cooldown_until),
  });
}

// ── 拖拽：正在拖哪一篇 + 落空时那句交代（拖到说不通的一列不能静默失败）──────
const dragging = ref<number | null>(null);
const dropHint = ref("");

function onDragStart(row: ManuscriptRow) {
  dragging.value = row.id;
}

function onDrop(to: ManuscriptStatus) {
  const id = dragging.value;
  dragging.value = null;
  const row = rows.value.find((item) => item.id === id);
  if (!row || row.status === to) return;
  if (to === "submitted") {
    dropHint.value = t("manuscript.drop_needs_record");
    return;
  }
  if (!canDrop(row.status, to)) {
    dropHint.value = t("manuscript.drop_illegal", {
      from: t(statusKey(row.status)),
      to: t(statusKey(to)),
    });
    return;
  }
  dropHint.value = "";
  void move(row.id, to);
}

// ── 记投稿 / 改记录：表单自己管字段，这里只管把这一篇交给它 ──────────────
function startSubmit(work_id: number) {
  openSubmit(work_id);
}

function startEdit(work_id: number, submission: Submission) {
  openEdit(work_id, submission);
}

/** 表单点了「记下」：新的一次 vs 改某一条，落点不同 */
function onSave(draft: SubmitDraft) {
  const target = form.value;
  if (!target) return;
  if (target.submission) void saveEdit(target.work_id, target.submission.id, draft);
  else void save(target.work_id, draft);
}

// ── 详情面板：看历史、定完稿目标 ────────────────────────────────────────
const targetInput = ref("");

function pick(row: ManuscriptRow) {
  targetInput.value = row.target_words === null ? "" : String(row.target_words);
  void select(row.id);
}

function closeDetail() {
  detail.value = null;
}

function saveTarget() {
  const row = selectedRow.value;
  if (!row) return;
  const text = targetInput.value.trim();
  const words = text === "" ? null : Number(text);
  void setTarget(row.id, Number.isFinite(words) ? words : null);
}

function confirmRemove(submission: Submission) {
  if (window.confirm(t("manuscript.delete_record_confirm", { venue: submission.venue }))) {
    void removeSubmission(submission.id);
  }
}

/** 改详情里那一条（用选中的那一篇，免得模板里到处写 `detail.work_id`）。 */
function editRecord(record: Submission) {
  const row = selectedRow.value;
  if (row) startEdit(row.id, record);
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

/** 判词看过了、想回去改一改：收起警告，表单还能改。 */
function dismissWarn() {
  warned.value = null;
}
</script>

<template>
  <div class="manuscript dialog" @click.self="close">
    <section class="manuscript__box dialog__box">
      <header class="manuscript__head dialog__head">
        <h2 class="manuscript__title dialog__title">{{ t("manuscript.title") }}</h2>
        <button
          type="button"
          class="manuscript__button dialog__button"
          :class="{ 'manuscript__button--on': view === 'board' }"
          @click="view = 'board'"
        >
          {{ t("manuscript.view_board") }}
        </button>
        <button
          type="button"
          class="manuscript__button dialog__button"
          :class="{ 'manuscript__button--on': view === 'unfinished' }"
          @click="view = 'unfinished'"
        >
          {{ t("manuscript.view_unfinished") }}
        </button>
        <button type="button" class="manuscript__button dialog__button" @click="close">
          {{ t("common.close") }}
        </button>
      </header>

      <p v-if="note" class="manuscript__note dialog__note">{{ note }}</p>
      <p v-if="dropHint" class="manuscript__note dialog__note">{{ dropHint }}</p>
      <!-- 超期未回：**一条提示**，不弹窗、不挡路——点卡片就能改投别家 -->
      <p v-if="overdueCount > 0" class="manuscript__note dialog__note">
        {{ t("manuscript.overdue_tip", { count: overdueCount }) }}
      </p>

      <div class="manuscript__body">
        <!-- 看板：七列，一列一个状态；卡片可以拖到别的列 -->
        <div v-if="view === 'board'" class="manuscript__board">
          <section
            v-for="column in BOARD_COLUMNS"
            :key="column"
            class="manuscript__col"
            @dragover.prevent
            @drop="onDrop(column)"
          >
            <header class="manuscript__col-head">
              <span>{{ t(statusKey(column)) }}</span>
              <span class="manuscript__count">{{ columns[column].length }}</span>
            </header>
            <ul class="manuscript__cards">
              <li
                v-for="row in columns[column]"
                :key="row.id"
                class="manuscript__card"
                :class="{
                  'manuscript__card--current': row.id === workId,
                  'manuscript__card--picked': row.id === detail?.work_id,
                }"
                draggable="true"
                @dragstart="onDragStart(row)"
                @click="pick(row)"
              >
                <span class="manuscript__name" :title="workLabel(row.title)">{{ workLabel(row.title) }}</span>
                <span class="manuscript__meta">{{ cardMeta(row) }}</span>
                <span v-if="row.active_venue" class="manuscript__active">{{ activeLine(row) }}</span>
                <span v-else-if="row.last_venue" class="manuscript__active">
                  {{ t("manuscript.last_venue", { venue: row.last_venue }) }}
                </span>
                <span class="manuscript__actions">
                  <button
                    type="button"
                    class="manuscript__button dialog__button"
                    :disabled="busy"
                    @click.stop="startSubmit(row.id)"
                  >
                    {{ t("manuscript.submit") }}
                  </button>
                  <button
                    type="button"
                    class="manuscript__button dialog__button"
                    :disabled="busy"
                    @click.stop="void openWork(row.id)"
                  >
                    {{ t("manuscript.open") }}
                  </button>
                </span>
              </li>
            </ul>
          </section>
        </div>

        <!-- 未完成散篇：只有设过完稿目标的才判得了；没定目标的单列一句提示 -->
        <div v-else class="manuscript__unfinished">
          <p class="manuscript__hint">{{ t("manuscript.unfinished.hint") }}</p>
          <ul class="manuscript__list">
            <li v-for="row in unfinished" :key="row.id" class="manuscript__row" @click="pick(row)">
              <span class="manuscript__name">{{ workLabel(row.title) }}</span>
              <span class="manuscript__meta">
                {{ t("manuscript.unfinished.remaining", {
                  words: remainingWords(row),
                  when: formatWhen(row.updated_at),
                }) }}
              </span>
              <button
                type="button"
                class="manuscript__button dialog__button"
                @click.stop="void openWork(row.id)"
              >
                {{ t("manuscript.open") }}
              </button>
            </li>
          </ul>
          <p v-if="unfinished.length === 0" class="manuscript__hint">
            {{ t("manuscript.unfinished.none") }}
          </p>
          <p v-if="untargeted.length > 0" class="manuscript__hint">
            {{ t("manuscript.unfinished.no_target", { count: untargeted.length }) }}
          </p>
        </div>

        <!-- 详情：选中那一篇的状态、完稿目标、全部投稿记录 -->
        <aside v-if="detail && selectedRow" class="manuscript__side">
          <header class="manuscript__side-head">
            <span class="manuscript__name">{{ workLabel(selectedRow.title) }}</span>
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
                <button
                  type="button"
                  class="manuscript__button dialog__button"
                  @click="editRecord(record)"
                >
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
      </div>

      <!-- 记一次投稿 / 改一条记录：同一个表单（字段与软拦都在它自己那一件里） -->
      <ManuscriptRecordForm
        v-if="form"
        :form="form"
        :default-cooldown-days="defaultCooldownDays"
        :busy="busy"
        :warned="warned"
        @save="onSave"
        @confirm="void confirmWarned()"
        @dismiss="dismissWarn()"
        @cancel="closeForm()"
      />
    </section>
  </div>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./manuscript-dialog.css"></style>
