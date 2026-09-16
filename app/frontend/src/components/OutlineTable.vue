<script setup lang="ts">
// 「章纲」那一页：**一整块主区的大表**——一章一行，行里的要素就在格子里填。
//
// 为什么是整块主区而不是弹窗里的小块（真机反馈 + 调研）：写大纲的动作是
// **顺着一列往下填**，小表单把这件事拆成了"开一次填一次"；Scrivener 的 Outliner
// 也正是占着编辑区的一整块，并且它自己就把这叫做"给喜欢用电子表格的人"。
//
// 这个文件只管长什么样；状态与命令在 [`useOutlineGrid`](./grid-panel.ts)，
// 纯逻辑三条各有各的文件：列与筛选走 [`grid.ts`](./grid.ts)、
// 整片粘贴走 [`grid-paste.ts`](./grid-paste.ts)、拖动落点走 [`drop-plan.ts`](./drop-plan.ts)。
//
// 手感五条：
// - 格子里直接打字；**Enter = 下一行同一列**（顺列往下填）、Tab 左右走（浏览器原生）；
// - 失焦即存，存的是"这一格"；
// - **整片粘贴**：从 Excel / WPS 粘进来按 Tab 与换行拆格，锚点就是点下去的那一格；
// - **按住章名那一格拖动**：调到别的卷里 / 换个位置（落点分上中下三段）；
// - 表尾 `+ 加一章`，卷可以收起。
//
// 页签的第二页（第一页是总纲）在 [`OutlineGrid`](./OutlineGrid.vue) 那个外壳里挂上来。
import { nextTick, ref, watch } from "vue";

import type { EditorSession } from "../editor/session.ts";
import { t } from "../locales/index.ts";
import { dropPlan, type DropZone } from "./drop-plan.ts";
import OutlineCastPicker from "./OutlineCastPicker.vue";
import {
  castText,
  cellEditable,
  cellValue,
  COLUMNS,
  columnWidth,
  foreshadowText,
  needsAttention,
  nextFillableRow,
  type GridColumn,
} from "./grid.ts";
import type { OutlineRowDto } from "../api/outline.ts";

const props = defineProps<{ session: EditorSession }>();
const { workId } = props.session;
const {
  rows,
  shown,
  dropRows,
  busy,
  errorText,
  columns,
  shownColumns,
  onlyUnfilled,
  collapsed,
  justSaved,
  castFor,
  castCards,
  toggleColumn,
  toggleCollapsed,
  saveSummary,
  saveField,
  paste,
  drop,
  openCast,
  closeCast,
  toggleCast,
  open,
  addChapter,
} = props.session.grid;

/** 表尾那个「+ 加一章」挂在哪：最后一个卷（没有卷就挂在根上）。 */
function lastVolume(): number | null {
  const volumes = rows.value.filter((row) => row.kind === "volume");
  return volumes.length > 0 ? volumes[volumes.length - 1].node_id : null;
}

function cellId(index: number, column: GridColumn): string {
  return `grid-${index}-${column}`;
}

/** Enter：**下一行同一列**（跳过填不了的行；到表尾就停在这儿）。 */
async function commitAndNext(event: KeyboardEvent, index: number, column: GridColumn) {
  const input = event.target as HTMLInputElement;
  if (column === "summary") await saveSummary(shown.value[index].node_id, input.value);
  await focusNext(index, column);
}

async function focusNext(index: number, column: GridColumn) {
  const next = nextFillableRow(shown.value, index, column, 1);
  if (next === null) return;
  const target = document.getElementById(cellId(next, column));
  if (target instanceof HTMLInputElement) {
    target.focus();
    target.select();
  }
}

/** Shift+Enter：往上走一格（同列）。 */
function focusPrev(index: number, column: GridColumn) {
  const prev = nextFillableRow(shown.value, index, column, -1);
  if (prev === null) return;
  const target = document.getElementById(cellId(prev, column));
  if (target instanceof HTMLInputElement) {
    target.focus();
    target.select();
  }
}

/** 一格存下去（失焦与 Enter 都走这儿）。 */
async function commit(index: number, column: GridColumn, value: string) {
  const row = shown.value[index];
  if (!row) return;
  if (column === "summary") {
    if (row.summary !== value) await saveSummary(row.node_id, value);
    return;
  }
  if (column === "pov" || column === "goal" || column === "conflict" || column === "outcome") {
    if (row.fields[column] !== value) await saveField(row.node_id, column, value);
  }
}

/**
 * 整片粘贴：**先把这一下拦下来**（不然浏览器会把它当普通文本塞进那一格）。
 *
 * 拆格与落点都在 `grid-paste.ts`；这里只负责"把这一刻的锚点（第几行第几列）交出去"。
 */
async function onPaste(event: ClipboardEvent, index: number, column: GridColumn) {
  const text = event.clipboardData?.getData("text/plain") ?? "";
  // 只有一格、又没有 Tab / 换行：那就是普通粘贴，交给浏览器（失焦时照旧逐格存）
  if (!text.includes("\t") && !text.includes("\n") && !text.includes("\r")) return;
  event.preventDefault();
  await paste({ row: index, column }, text);
}

/** 选人卡开 / 收（点同一行就是收起来）。 */
function toggleCastCard(node_id: number) {
  if (castFor.value === node_id) closeCast();
  else void openCast(node_id);
}

/**
 * 点在表里别处：**把选人卡收起来**。
 *
 * 点在这一格里面（勾人 / 收起按钮）不动它——不然勾一下卡片就没了，
 * 连选几个人这件事根本做不成。
 */
function onTableClick(event: MouseEvent) {
  if (castFor.value === null) return;
  const target = event.target as HTMLElement | null;
  if (target?.closest(".grid__cast")) return;
  closeCast();
}

// ── 拖行：上三分之一排前面、下三分之一排后面、中间放进卷里 ──────────────
const dragging = ref<number | null>(null);
const dropOn = ref<number | null>(null);
const dropZone = ref<DropZone>("inside");

function onDragStart(row: OutlineRowDto, event: DragEvent) {
  dragging.value = row.node_id;
  event.dataTransfer?.setData("text/plain", String(row.node_id));
  if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
}

function resetDrag() {
  dragging.value = null;
  dropOn.value = null;
  dropZone.value = "inside";
}

function onDragOver(row: OutlineRowDto, event: DragEvent) {
  const id = dragging.value;
  if (id === null || id === row.node_id) return;
  const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
  const ratio = (event.clientY - box.top) / Math.max(1, box.height);
  let zone: DropZone = ratio < 0.3 ? "before" : ratio > 0.7 ? "after" : "inside";
  // 只有卷收得下东西；别的行中间那一段退成"排在前后"，别画一个骗人的落点
  if (zone === "inside" && row.kind !== "volume") zone = ratio < 0.5 ? "before" : "after";
  // 会成环（拖进自己那一支）就整个不画落点
  if (dropPlan(dropRows.value, id, row.node_id, zone) === null) {
    dropOn.value = null;
    return;
  }
  dropOn.value = row.node_id;
  dropZone.value = zone;
}

async function onDrop(row: OutlineRowDto) {
  const id = dragging.value;
  const zone = dropZone.value;
  const landed = dropOn.value;
  resetDrag();
  if (id === null || landed !== row.node_id || id === row.node_id) return;
  await drop(id, row.node_id, zone);
}

/** 把焦点放到**第一格能填的格子**上：进来就能顺着往下填。 */
async function focusFirstCell() {
  await nextTick();
  const first = shownColumns.value.find((spec) => spec.key === "summary" && spec.editable);
  if (!first) return;
  const index = shown.value.findIndex((row) => cellEditable(first, row));
  if (index < 0) return;
  const target = document.getElementById(cellId(index, first.key));
  if (target instanceof HTMLInputElement) target.focus();
}

/**
 * 这一页挂上来、或者表刚读回来时，把焦点放进第一格——**只做一次**。
 *
 * 为什么等数据：切页时这一页是先挂上、表后到（`load()` 是异步的），
 * 挂载那一刻 `shown` 还是空的（0.68.1 切页时抓的）。
 * 顺带让 Esc 有地方落（Esc 的监听在外壳那一层——全局那套在"有弹窗开着"时一律不认，
 * 见 `editor/shortcuts.ts` 的四条分寸）。
 */
const focusedOnce = ref(false);
watch(
  () => shown.value.length,
  (count) => {
    if (count === 0 || focusedOnce.value) return;
    focusedOnce.value = true;
    void focusFirstCell();
  },
  { immediate: true },
);
</script>

<template>
  <p class="grid__rule">{{ t("grid.rule") }}</p>
  <p v-if="errorText" class="grid__error">{{ errorText }}</p>
  <p v-if="workId === null" class="grid__hint">{{ t("grid.no_work") }}</p>

  <!-- 这一页自己的工具栏：只看没补的 / 加一章 / 露哪几列（都不落盘） -->
  <nav class="grid__columns">
    <span class="grid__hint">{{ t("grid.columns") }}</span>
    <label v-for="spec in COLUMNS" :key="spec.key" class="grid__column">
      <input
        type="checkbox"
        :checked="columns.includes(spec.key)"
        @change="toggleColumn(spec.key)"
      />
      <span>{{ t(`grid.col.${spec.key}`) }}</span>
    </label>
    <label class="grid__toggle grid__toggle--push">
      <input v-model="onlyUnfilled" type="checkbox" />
      <span>{{ t("grid.only_unfilled") }}</span>
    </label>
    <button
      type="button"
      class="dialog__button"
      :disabled="busy"
      @click="void addChapter(lastVolume())"
    >
      {{ t("grid.add_chapter") }}
    </button>
  </nav>

  <div v-if="shown.length > 0" class="grid__scroll">
    <table class="grid__table" @click="onTableClick">
      <thead>
        <tr>
          <th
            v-for="spec in shownColumns"
            :key="spec.key"
            :style="{ width: columnWidth(spec.key) }"
            scope="col"
          >
            {{ t(`grid.col.${spec.key}`) }}
          </th>
        </tr>
      </thead>
      <tbody>
        <template v-for="(row, index) in shown" :key="row.node_id">
          <!-- 卷：分组行（不占格，可收起；拖到它中间＝放进这一卷） -->
          <tr
            v-if="row.kind === 'volume'"
            class="grid__group"
            :class="dropOn === row.node_id ? `grid__drop--${dropZone}` : ''"
            draggable="true"
            @dragstart="onDragStart(row, $event)"
            @dragover.prevent="onDragOver(row, $event)"
            @drop.prevent="void onDrop(row)"
            @dragend="resetDrag()"
          >
            <td :colspan="shownColumns.length">
              <button type="button" class="grid__fold" @click="toggleCollapsed(row.node_id)">
                {{ collapsed.has(row.node_id) ? "▸" : "▾" }}
              </button>
              <span class="grid__group-title">
                {{ row.title.trim() === "" ? t("grid.untitled_volume") : row.title }}
              </span>
            </td>
          </tr>

          <!-- 一行一个正文单位（章 / 节 / 场景卡） -->
          <tr
            v-else
            class="grid__row"
            :class="[
              { 'grid__row--todo': needsAttention(row) },
              dropOn === row.node_id ? `grid__drop--${dropZone}` : '',
            ]"
            @dragover.prevent="onDragOver(row, $event)"
            @drop.prevent="void onDrop(row)"
            @dragend="resetDrag()"
          >
            <td
              v-for="spec in shownColumns"
              :key="spec.key"
              :style="{ paddingLeft: `${0.4 + row.depth * 0.9}rem` }"
            >
              <!-- 章名：点一下跳正文；**按住这一格拖动**就是给这一行换位置 -->
              <button
                v-if="spec.key === 'title'"
                type="button"
                class="grid__open"
                :title="t('grid.open')"
                draggable="true"
                @dragstart="onDragStart(row, $event)"
                @click="void open(row.node_id)"
              >
                {{ row.title.trim() === "" ? t("grid.untitled") : row.title }}
              </button>

              <!-- 出场人物：点开选人（表里唯一不是打字改的一格） -->
              <div v-else-if="spec.key === 'cast'" class="grid__cast">
                <button
                  type="button"
                  class="grid__cast-open"
                  :class="{ 'grid__cast-open--empty': row.cast.length === 0 }"
                  :title="t('grid.cast.title')"
                  @click="toggleCastCard(row.node_id)"
                >
                  {{ row.cast.length === 0 ? t("grid.cast.pick") : castText(row) }}
                </button>
                <OutlineCastPicker
                  v-if="castFor === row.node_id"
                  :cards="castCards"
                  :selected="row.cast"
                  :busy="busy"
                  @toggle="(entity_id) => void toggleCast(row.node_id, entity_id)"
                  @close="closeCast()"
                />
              </div>

              <!-- 伏笔 / 字数：核心算的，只读 -->
              <span v-else-if="spec.key === 'foreshadow'" class="grid__cell-text">
                {{ foreshadowText(row) }}
              </span>
              <span v-else-if="spec.key === 'words'" class="grid__cell-text">
                {{ row.word_count }}
              </span>

              <!-- 可填的格：一句话与四格（粘一整片也从这儿进） -->
              <input
                v-else-if="cellEditable(spec, row)"
                :id="cellId(index, spec.key)"
                class="grid__cell"
                type="text"
                :value="cellValue(row, spec.key)"
                :disabled="busy"
                @paste="onPaste($event, index, spec.key)"
                @blur="commit(index, spec.key, ($event.target as HTMLInputElement).value)"
                @keydown.enter.exact.prevent="
                  commitAndNext($event, index, spec.key)
                "
                @keydown.shift.enter.prevent="focusPrev(index, spec.key)"
              />
              <span v-else class="grid__cell-text">—</span>
            </td>
          </tr>
        </template>
      </tbody>
    </table>
  </div>
  <p v-else-if="busy" class="grid__hint">{{ t("grid.loading") }}</p>
  <p v-else class="grid__hint">
    {{ onlyUnfilled ? t("grid.nothing_unfilled") : t("grid.empty") }}
  </p>

  <p v-if="justSaved" class="grid__hint">{{ justSaved }}</p>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./outline-table.css"></style>
