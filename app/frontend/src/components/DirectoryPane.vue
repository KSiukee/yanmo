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
  refresh: refreshTree,
} = props.session.directory;
const { neighbors, switching, switchChapter, addChapterAfter, deleteNode } = props.session;
const { trash, gaps } = props.session;
const { pending: gapPending, busy: gapBusy, check: checkGap, fill: fillGap, answer: answerGap } = gaps;

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

/** 弹出"这里少了一章"那一问的那一行：作者答完，接着按他点「+」的意图建章 */
const gapRow = ref<TreeRow | null>(null);

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
  const includes = row.accepts_children ? "里面的内容会一起进回收站，" : "";
  if (window.confirm(`删掉「${row.title}」？${includes}之后能在回收站里捞回来。`)) {
    void deleteNode(row.id);
  }
}

/** 行上的「+」：先问一嘴"这一层是不是少了一章"，没得问就直接建。 */
async function addHere(row: TreeRow) {
  // 新章落在哪一层：容器往里加，章就是它自己那一层（跟 addIntent 的分支一一对应）
  const layer = addIntent(row) === "inside" ? row.id : row.parent_id;
  if (await checkGap(layer)) {
    gapRow.value = row; // 有该问的空缺：先摆弹窗，等作者拿主意再建新章
    return;
  }
  await addByIntent(row);
}

/** 作者原来点「+」的意图：容器往里加一章，章就接着它往后插一章。 */
async function addByIntent(row: TreeRow) {
  if (addIntent(row) === "inside") {
    // 标题留空＝由核心按同层序号取名
    const created = await create(row.id, "chapter", "");
    if (created !== null) await select(created);
    return;
  }
  await addChapterAfter(row.id); // 走核心既有的"插在这一章之后"
}

/** 补写：在原位新建一个空章（不是恢复旧稿），补完直接开写。 */
async function onGapFill() {
  gapRow.value = null;
  const created = await fillGap();
  if (created === null) return;
  await refreshTree(); // 补出来的章得看得见
  await select(created);
}

/** 「稍后再说 / 不用了」：记下答复，再照他原来点的意图接着建新章。 */
async function onGapAnswer(answer: "deferred" | "ignored") {
  const row = gapRow.value;
  if (!(await answerGap(answer))) return; // 答复记不下来就别偷偷往下走
  gapRow.value = null;
  if (row) await addByIntent(row);
}

/** 「去回收站看看」：他还没拿主意，**什么都不记**，下次点「+」还会问。 */
function onGapTrash() {
  onGapClose();
  trash.toggle();
}

/** 点遮罩关掉：同样不记答复——只是这次不想答，不该被当成「不用了」 */
function onGapClose() {
  gapRow.value = null;
  gaps.dismiss();
}
</script>

<template>
  <aside class="pane">
    <header class="pane__head">
      <h2 class="pane__title">目录</h2>
      <button
        type="button"
        class="pane__button"
        title="新建一卷"
        :disabled="switching"
        @click="create(null, 'volume', '')"
      >
        + 卷
      </button>
    </header>

    <p v-if="hasVolumes" class="pane__setup">
      <label title="每卷大概写几章：只影响目录里「本卷 12/30 章」这行小字，不会改你的结构">
        每卷
        <input
          class="pane__target"
          type="number"
          min="1"
          :value="volumeTarget ?? ''"
          placeholder="—"
          @change="saveVolumeTarget"
        />
        章
      </label>
    </p>

    <p v-if="rows.length === 0" class="pane__empty">还没有目录</p>
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
          :title="row.expanded ? '收起' : '展开'"
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
        <span v-else class="tree__title" :title="row.title">{{ row.title || "（未命名）" }}</span>

        <span v-if="row.holds_body" class="tree__words">{{ row.has_body ? formatWords(row.word_count) : "空" }}</span>
        <span v-else class="tree__words" :title="`本卷 ${row.chapter_count} 章 · ${row.subtree_word_count} 字`">
          {{ containerLabel(row, volumeTarget) }}
        </span>
        <button
          v-if="addIntent(row)"
          type="button"
          class="tree__add"
          :title="addIntent(row) === 'inside' ? '往里新建一章' : '在它后面新建一章'"
          @click.stop="addHere(row)"
        >
          +
        </button>
        <button
          type="button"
          class="tree__del"
          title="删掉它（会进回收站，可以捞回来）"
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
        :title="neighbors?.previous ? `上一章：${neighbors.previous.title}` : '已经是第一章'"
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
        :title="neighbors?.next ? `下一章：${neighbors.next.title}` : '已经是最后一章'"
        @click="switchChapter(neighbors?.next?.id)"
      >
        →
      </button>
    </footer>

    <GapDialog
      v-if="gapPending && gapRow"
      :gap="gapPending"
      :busy="gapBusy"
      @fill="onGapFill"
      @defer="onGapAnswer('deferred')"
      @ignore="onGapAnswer('ignored')"
      @trash="onGapTrash"
      @close="onGapClose"
    />
  </aside>
</template>

<style scoped src="./directory-pane.css"></style>
