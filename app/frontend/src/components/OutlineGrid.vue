<script setup lang="ts">
// 「大纲」表：**一整块主区的大表**——一章一行，行里的要素就在格子里填。
//
// 为什么是整块主区而不是弹窗里的小块（真机反馈 + 调研）：写大纲的动作是
// **顺着一列往下填**，小表单把这件事拆成了"开一次填一次"；Scrivener 的 Outliner
// 也正是占着编辑区的一整块，并且它自己就把这叫做"给喜欢用电子表格的人"。
//
// 这个文件只管长什么样；状态与命令在 [`useOutlineGrid`](./grid-panel.ts)，
// 纯逻辑（列、筛选、键盘往哪走）在 [`grid.ts`](./grid.ts)。
//
// 手感三条：
// - 格子里直接打字；**Enter = 下一行同一列**（顺列往下填）、Tab 左右走（浏览器原生）；
// - 失焦即存，存的是"这一格"；
// - 表尾 `+ 加一章`，卷可以收起。
import { computed, nextTick, watch } from "vue";

import type { EditorSession } from "../editor/session.ts";
import { t } from "../locales/index.ts";
import {
  cellEditable,
  cellValue,
  COLUMNS,
  columnWidth,
  foreshadowText,
  needsAttention,
  nextFillableRow,
  visibleRows,
  type GridColumn,
} from "./grid.ts";

const props = defineProps<{ session: EditorSession }>();
const { workId } = props.session;
const {
  visible,
  rows,
  busy,
  errorText,
  columns,
  onlyUnfilled,
  collapsed,
  justSaved,
  hide,
  toggleColumn,
  toggleCollapsed,
  saveSummary,
  saveField,
  open,
  addChapter,
} = props.session.grid;

/** 表里要摆的行（折叠 + 只看没填的）。 */
const shown = computed(() =>
  visibleRows(rows.value, { collapsed: collapsed.value, onlyUnfilled: onlyUnfilled.value }),
);

/** 露出来的列（保持 COLUMNS 的固定顺序）。 */
const shownColumns = computed(() => COLUMNS.filter((spec) => columns.value.includes(spec.key)));

/** 表尾那个「+ 加一章」挂在哪：最后一个卷（没有卷就挂在根上）。 */
const lastVolume = computed(() => {
  const volumes = rows.value.filter((row) => row.kind === "volume");
  return volumes.length > 0 ? volumes[volumes.length - 1].node_id : null;
});

function cellId(index: number, column: GridColumn): string {
  return `grid-${index}-${column}`;
}

/** Enter：**下一行同一列**（跳过填不了的行；到表尾就新建一章接着填）。 */
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
 * 打开时把焦点放到**第一格能填的格子**上：一进来就能打字。
 *
 * 顺带让 Esc 有地方落（Esc 的监听在表格这一层——全局那套在"有弹窗开着"时一律不认，
 * 见 `editor/shortcuts.ts` 的四条分寸）。
 */
watch(visible, async (on) => {
  if (!on) return;
  await nextTick();
  const first = shownColumns.value.find((spec) => spec.key === "summary" && spec.editable);
  if (!first) return;
  const index = shown.value.findIndex((row) => cellEditable(first, row));
  if (index < 0) return;
  const target = document.getElementById(cellId(index, first.key));
  if (target instanceof HTMLInputElement) target.focus();
});
</script>

<template>
  <div v-if="visible" class="grid dialog" @click.self="hide()">
    <section class="grid__box dialog__box" @keydown.esc.stop="hide()">
      <header class="grid__head dialog__head">
        <h2 class="grid__title dialog__title">{{ t("grid.title") }}</h2>
        <label class="grid__toggle">
          <input v-model="onlyUnfilled" type="checkbox" />
          <span>{{ t("grid.only_unfilled") }}</span>
        </label>
        <button type="button" class="dialog__button" :disabled="busy" @click="void addChapter(lastVolume)">
          {{ t("grid.add_chapter") }}
        </button>
        <button type="button" class="dialog__button" @click="hide()">{{ t("common.close") }}</button>
      </header>

      <p class="grid__rule">{{ t("grid.rule") }}</p>
      <p v-if="errorText" class="grid__error">{{ errorText }}</p>
      <p v-if="workId === null" class="grid__hint">{{ t("grid.no_work") }}</p>

      <!-- 列开关：表里要摆哪几列（不落盘：这是"当下想看什么"） -->
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
      </nav>

      <div v-if="shown.length > 0" class="grid__scroll">
        <table class="grid__table">
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
              <!-- 卷：分组行（不占格，可收起） -->
              <tr v-if="row.kind === 'volume'" class="grid__group">
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
              <tr v-else class="grid__row" :class="{ 'grid__row--todo': needsAttention(row) }">
                <td
                  v-for="spec in shownColumns"
                  :key="spec.key"
                  :style="{ paddingLeft: `${0.4 + row.depth * 0.9}rem` }"
                >
                  <!-- 章名：点一下跳正文 -->
                  <button
                    v-if="spec.key === 'title'"
                    type="button"
                    class="grid__open"
                    :title="t('grid.open')"
                    @click="void open(row.node_id)"
                  >
                    {{ row.title.trim() === "" ? t("grid.untitled") : row.title }}
                  </button>

                  <!-- 伏笔 / 字数：核心算的，只读 -->
                  <span v-else-if="spec.key === 'foreshadow'" class="grid__cell-text">
                    {{ foreshadowText(row) }}
                  </span>
                  <span v-else-if="spec.key === 'words'" class="grid__cell-text">
                    {{ row.word_count }}
                  </span>

                  <!-- 可填的格：一句话与四格 -->
                  <input
                    v-else-if="cellEditable(spec, row)"
                    :id="cellId(index, spec.key)"
                    class="grid__cell"
                    type="text"
                    :value="cellValue(row, spec.key)"
                    :disabled="busy"
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

      <p v-if="justSaved" class="grid__hint">{{ t("grid.saved") }}</p>
    </section>
  </div>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./outline-grid.css"></style>
