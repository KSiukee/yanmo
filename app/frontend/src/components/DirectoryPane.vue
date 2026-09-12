<script setup lang="ts">
// 目录树：卷 / 章的**看得见、点得动、拖得走**。
//
// 这个文件只管"长什么样、怎么点"：树的状态在 editor/tree.ts，接线在 editor/directory.ts，
// 落盘与切章的纪律在 editor/session.ts——**视图不掺业务**。
//
// 三件事在这里收口：
// - 懒加载：只有展开过的层级才画出来（长篇几百章不会一次全进来）；
// - 拖拽排序：落在行的上 / 中 / 下三段，分别是"排到它前面 / 放进它里面 / 排到它后面"；
// - 内联改名：双击就地改，回车落、Esc 撤（空标题＝放弃）。
import { computed, nextTick, ref } from "vue";

import type { EditorSession } from "../editor/session";
import { addIntent, containerLabel, type TreeRow } from "../editor/tree";
import { t } from "../locales/index.ts";
import { formatWords } from "../editor/display.ts";
import GapDialog from "./GapDialog.vue";

const props = defineProps<{ session: EditorSession }>();
// 从会话对象里取出的都是 ref，模板里照常自动解包
const {
  rows,
  current,
  toggle,
  select,
  rename,
  move,
  create,
  canDrop,
  volumeTarget,
  setVolumeTarget,
} = props.session.directory;
const { neighbors, switching, switchChapter, deleteNode, adding, trash } = props.session;
// 点「+」之后的编排在 editor/add-chapter.ts（这里只接线，不写流程）
const {
  visible: gapVisible,
  gap: gapPending,
  busy: gapBusy,
  addHere,
  fill: fillGap,
  answer: answerGap,
  dismiss: dismissGap,
} = adding;

/**
 * 同层序号（1 起）：**没起名的卷靠它显示「第 1 卷」**，而不是一个冷冰冰的"未命名"。
 *
 * 序号按当前显示顺序现算（拖动之后自然跟着变），不去库里存一份会打架的副本。
 */
const serialById = computed(() => {
  const seen = new Map<number | null, number>();
  const out = new Map<number, number>();
  for (const row of rows.value) {
    const next = (seen.get(row.parent_id) ?? 0) + 1;
    seen.set(row.parent_id, next);
    out.set(row.id, next);
  }
  return out;
});

/** 目录里显示的名字：没起名的容器按序号补"第 N 卷"，其它没起名的显示"（未命名）" */
function rowLabel(row: TreeRow): string {
  if (row.title) return row.title;
  if (row.accepts_children && !row.holds_body) {
    return t("tree.volume_placeholder", { n: serialById.value.get(row.id) ?? 1 });
  }
  return t("common.untitled_full");
}

/** 有卷才显示"每卷多少章"这一栏：零层级作品用不上它 */
const hasVolumes = computed(() => rows.value.some((row) => row.accepts_children && !row.holds_body));

/** 改卷长：空着或 ≤0 就当作"没设过"（清掉） */
function saveVolumeTarget(event: Event) {
  const input = event.target as HTMLInputElement;
  const parsed = Number.parseInt(input.value, 10);
  void setVolumeTarget(Number.isFinite(parsed) && parsed > 0 ? parsed : null);
}

/** 正在改名的那一行（同时只可能有一行） */
const editing = ref<number | null>(null);
const draft = ref("");
const listEl = ref<HTMLElement | null>(null);

/** 拖拽：谁在拖、落在谁身上、落在哪一段 */
const dragging = ref<number | null>(null);
const dropOn = ref<number | null>(null);
const dropZone = ref<"before" | "inside" | "after">("inside");

/** 点一行：能写的就打开来写，容器就展开 / 收起（**类型说了算，界面不猜**） */
function openRow(row: TreeRow) {
  if (row.holds_body) void select(row.id);
  else void toggle(row.id);
}

async function startRename(row: TreeRow) {
  editing.value = row.id;
  draft.value = row.title;
  await nextTick();
  const input = listEl.value?.querySelector<HTMLInputElement>(".tree__rename");
  input?.focus();
  input?.select();
}

async function commitRename(row: TreeRow) {
  if (editing.value !== row.id) return;
  editing.value = null;
  await rename(row.id, draft.value);
}

function onDragStart(row: TreeRow, event: DragEvent) {
  dragging.value = row.id;
  event.dataTransfer?.setData("text/plain", String(row.id));
  if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
}

function onDragOver(row: TreeRow, event: DragEvent) {
  const id = dragging.value;
  if (id === null || id === row.id) return;
  const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
  const ratio = (event.clientY - box.top) / Math.max(1, box.height);
  let zone: "before" | "inside" | "after" = ratio < 0.25 ? "before" : ratio > 0.75 ? "after" : "inside";
  // 放不进去（收不了下级 / 会成环）就退成"排在前后"，别画一个骗人的落点
  if (zone === "inside" && (!row.accepts_children || !canDrop(id, row.id))) {
    zone = ratio < 0.5 ? "before" : "after";
  }
  dropOn.value = row.id;
  dropZone.value = zone;
}

function resetDrag() {
  dragging.value = null;
  dropOn.value = null;
  dropZone.value = "inside";
}

async function onDrop(row: TreeRow) {
  const id = dragging.value;
  const zone = dropZone.value;
  const landed = dropOn.value;
  resetDrag();
  if (id === null || landed !== row.id || id === row.id) return;

  if (zone === "inside") {
    await move(id, row.id, Number.MAX_SAFE_INTEGER); // 追加到这一层末尾（越界由核心夹）
    return;
  }
  const siblings = rows.value.filter((item) => item.parent_id === row.parent_id);
  const at = siblings.findIndex((item) => item.id === row.id);
  if (at < 0) return;
  await move(id, row.parent_id, at + (zone === "after" ? 1 : 0));
}

/** 行上的「×」：删掉它（软删，进回收站能捞回来）——容器会把里面的东西一起带走 */
function askDelete(row: TreeRow) {
  const includes = row.accepts_children ? t("tree.delete_children_note") : "";
  const question = t("tree.delete_confirm", { title: rowLabel(row), children: includes });
  if (window.confirm(question)) {
    void deleteNode(row.id);
  }
}

/** 去回收站看看：先把这一问收起来（**不记答复**），再打开回收站 */
function goTrash() {
  dismissGap();
  trash.toggle();
}
</script>

<template>
  <aside class="pane">
    <header class="pane__head">
      <h2 class="pane__title">{{ t("tree.title") }}</h2>
      <button
        type="button"
        class="pane__button"
        :title="t('tree.new_volume')"
        :disabled="switching"
        @click="create(null, 'volume', '')"
      >
        {{ t("tree.new_volume_button") }}
      </button>
    </header>

    <p v-if="hasVolumes" class="pane__setup">
      <label :title="t('tree.volume_target_title')">
        {{ t("tree.volume_target_prefix") }}
        <input
          class="pane__target"
          type="number"
          min="1"
          :value="volumeTarget ?? ''"
          placeholder="—"
          @change="saveVolumeTarget"
        />
        {{ t("tree.volume_target_suffix") }}
      </label>
    </p>

    <p v-if="rows.length === 0" class="pane__empty">{{ t("tree.empty") }}</p>
    <ul ref="listEl" class="tree">
      <li
        v-for="row in rows"
        :key="row.id"
        class="tree__row"
        :class="[
          { 'tree__row--current': row.id === current },
          dropOn === row.id ? `tree__row--${dropZone}` : '',
        ]"
        :style="{ paddingLeft: `${6 + row.depth * 14}px` }"
        :draggable="editing === row.id ? 'false' : 'true'"
        @click="openRow(row)"
        @dblclick="startRename(row)"
        @dragstart="onDragStart(row, $event)"
        @dragover.prevent="onDragOver(row, $event)"
        @drop.prevent="onDrop(row)"
        @dragend="resetDrag"
      >
        <button
          type="button"
          class="tree__arrow"
          :class="{ 'tree__arrow--none': !row.has_children }"
          :title="row.expanded ? t('tree.collapse') : t('tree.expand')"
          @click.stop="toggle(row.id)"
        >
          {{ row.expanded ? "▾" : "▸" }}
        </button>

        <input
          v-if="editing === row.id"
          v-model="draft"
          class="tree__rename"
          type="text"
          @click.stop
          @keydown.enter="commitRename(row)"
          @keydown.esc="editing = null"
          @blur="commitRename(row)"
        />
        <span v-else class="tree__title" :title="rowLabel(row)">{{ rowLabel(row) }}</span>

        <span v-if="row.holds_body" class="tree__words">{{ row.has_body ? formatWords(row.word_count) : t("tree.empty_chapter") }}</span>
        <span v-else class="tree__words" :title="t('tree.volume_stat_title', { chapters: row.chapter_count, words: row.subtree_word_count })">
          {{ containerLabel(row, volumeTarget) }}
        </span>
        <button
          v-if="addIntent(row)"
          type="button"
          class="tree__add"
          :title="addIntent(row) === 'inside' ? t('tree.add_inside') : t('tree.add_after')"
          @click.stop="addHere(row)"
        >
          +
        </button>
        <button
          type="button"
          class="tree__del"
          :title="t('tree.delete_title')"
          @click.stop="askDelete(row)"
        >
          ×
        </button>
      </li>
    </ul>

    <footer class="pane__foot">
      <button
        type="button"
        class="pane__button"
        :disabled="switching || !neighbors?.previous"
        :title="neighbors?.previous ? t('tree.prev_chapter', { title: neighbors.previous.title }) : t('tree.at_first')"
        @click="switchChapter(neighbors?.previous?.id)"
      >
        ←
      </button>
      <span class="pane__pos">
        {{ neighbors ? `${neighbors.index} / ${neighbors.total}` : "—" }}
      </span>
      <button
        type="button"
        class="pane__button"
        :disabled="switching || !neighbors?.next"
        :title="neighbors?.next ? t('tree.next_chapter', { title: neighbors.next.title }) : t('tree.at_last')"
        @click="switchChapter(neighbors?.next?.id)"
      >
        →
      </button>
    </footer>

    <GapDialog
      v-if="gapVisible"
      :gap="gapPending"
      :busy="gapBusy"
      @fill="fillGap"
      @defer="answerGap('deferred')"
      @ignore="answerGap('ignored')"
      @trash="goTrash"
      @close="dismissGap"
    />
  </aside>
</template>

<style scoped src="./directory-pane.css"></style>
