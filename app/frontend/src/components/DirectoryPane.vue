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
import { composeTitle, renderedPrefix, splitTitle, type TitleParts } from "../editor/title-edit";
import { t } from "../locales/index.ts";
import { formatCaliberNumber, formatCaliberWords } from "../editor/display.ts";
import { dropPlan, type DropZone } from "./drop-plan.ts";

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
  plan,
  setVolumeTarget,
} = props.session.directory;
// 目录树里的字数按**当前口径**显示（口径跟着作品语言，可在状态栏切换）
const { caliber } = props.session;
const { neighbors, switching, switchChapter, deleteNode, adding, trash } = props.session;
// 点「+」之后的编排在 editor/add-chapter.ts（这里只接线，不写流程）
const { addHere } = adding;
// 分卷：到点了提一句、收好之后能撤销（编排与分寸在 editor/volumes.ts）
const { offer: volumeOffer, closed: closedVolume, closeHere, snooze, undo } =
  props.session.directory.volumes;

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
  // 显示一律走**渲染后**的标题（`第{$N}章` → `第3章`）；改名时才编辑 `row.title` 原文
  if (row.title_rendered) return row.title_rendered;
  if (row.accepts_children && !row.holds_body) {
    return t("tree.volume_placeholder", { n: serialById.value.get(row.id) ?? 1 });
  }
  return t("common.untitled_full");
}

/** 有卷才显示"每卷多少章"这一栏：零层级作品用不上它 */
const hasVolumes = computed(() => rows.value.some((row) => row.accepts_children && !row.holds_body));

/**
 * 目录里分母用哪个数：**学到的那个阈值说了算**（核心那边口径只有一份）。
 *
 * 只显示作者手填的那个数的话，会出现「本卷 26/30 章」旁边却问
 * "要不要在 27 章收卷"——同一个界面上两个数打架，作者只会觉得软件在瞎猜。
 */
const threshold = computed(() => plan.value?.effective ?? null);

/** 收卷那句话：按"提前 / 正好 / 延后"换话术，并把阈值是从哪来的说清楚 */
const offerText = computed(() => {
  const offer = volumeOffer.value;
  if (!offer) return "";
  const target = offer.plan.effective ?? offer.count;
  const source =
    offer.plan.learned !== null
      ? t("tree.volume_source_learned", { target })
      : t("tree.volume_source_target", { target });
  const key =
    offer.phase === "early"
      ? "tree.volume_offer_early"
      : offer.phase === "late"
        ? "tree.volume_offer_late"
        : "tree.volume_offer_on_target";
  return t(key, { count: offer.count, source });
});

/** 改卷长：空着或 ≤0 就当作"没设过"（清掉） */
function saveVolumeTarget(event: Event) {
  const input = event.target as HTMLInputElement;
  const parsed = Number.parseInt(input.value, 10);
  void setVolumeTarget(Number.isFinite(parsed) && parsed > 0 ? parsed : null);
}

/** 正在改名的那一行（同时只可能有一行） */
const editing = ref<number | null>(null);
const draft = ref("");
/** 改名时那一行的**宏骨架**（`第{$N}章 `）与它渲染后的样子（`第1章`）——都不给作者编辑 */
const editingParts = ref<TitleParts>({ prefix: "", name: "" });
const editingPrefix = ref("");
const listEl = ref<HTMLElement | null>(null);

/** 拖拽：谁在拖、落在谁身上、落在哪一段 */
const dragging = ref<number | null>(null);
const dropOn = ref<number | null>(null);
const dropZone = ref<DropZone>("inside");

/** 点一行：能写的就打开来写，容器就展开 / 收起（**类型说了算，界面不猜**） */
function openRow(row: TreeRow) {
  if (row.holds_body) void select(row.id);
  else void toggle(row.id);
}

async function startRename(row: TreeRow) {
  editing.value = row.id;
  // 输入框里**只放名字**：宏骨架（`第{$N}章`）留在前面当灰字提示，渲染后的号由核心算
  const parts = splitTitle(row.title);
  editingParts.value = parts;
  editingPrefix.value = renderedPrefix(rowLabel(row), parts);
  draft.value = parts.name;
  await nextTick();
  const input = listEl.value?.querySelector<HTMLInputElement>(".tree__rename");
  input?.focus();
  input?.select();
}

async function commitRename(row: TreeRow) {
  if (editing.value !== row.id) return;
  editing.value = null;
  // 骨架原样写回 + 作者改的名字（宏一个字符都不动，编号不会因为改名丢）
  await rename(row.id, composeTitle(editingParts.value, draft.value));
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
  let zone: DropZone = ratio < 0.25 ? "before" : ratio > 0.75 ? "after" : "inside";
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

  // 落点 → (新父级, 第几位)：算法只有一处（`drop-plan.ts`），目录树与大纲表共用
  const landing = dropPlan(
    rows.value.map((item) => ({ id: item.id, parent_id: item.parent_id })),
    id,
    row.id,
    zone,
  );
  if (landing === null) return;
  await move(id, landing.parent_id, landing.index);
}

/** 行上的「×」：删掉它（软删，进回收站能捞回来）——容器会把里面的东西一起带走 */
function askDelete(row: TreeRow) {
  const includes = row.accepts_children ? t("tree.delete_children_note") : "";
  const question = t("tree.delete_confirm", { title: rowLabel(row), children: includes });
  if (window.confirm(question)) {
    void deleteNode(row.id);
  }
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

    <!-- 到收卷点了：安静地问一句，两个按钮就是全部（绝不自动改结构） -->
    <p v-if="offerText" class="pane__volume">
      <span class="pane__volume-text">{{ offerText }}</span>
      <span class="pane__volume-actions">
        <button
          type="button"
          class="pane__button"
          :title="t('tree.volume_close_here_title')"
          @click="closeHere"
        >
          {{ t("tree.volume_close_here") }}
        </button>
        <button
          type="button"
          class="pane__button"
          :title="t('tree.volume_wait_title')"
          @click="snooze"
        >
          {{ t("tree.volume_wait") }}
        </button>
      </span>
    </p>

    <!-- 刚收好一卷：一句回执 + 一个撤销出口（撤了正文一个字都不动） -->
    <p v-if="closedVolume" class="pane__volume pane__volume--done">
      <span class="pane__volume-text">
        {{ t("tree.volume_closed", { count: closedVolume.count }) }}
      </span>
      <span class="pane__volume-actions">
        <button
          type="button"
          class="pane__button"
          :title="t('tree.volume_undo_title')"
          @click="undo"
        >
          {{ t("tree.volume_undo") }}
        </button>
      </span>
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

        <span v-if="editing === row.id" class="tree__rename-box">
          <span v-if="editingPrefix" class="tree__rename-prefix">{{ editingPrefix }}</span>
          <input
            v-model="draft"
            class="tree__rename"
            type="text"
            @click.stop
            @keydown.enter="commitRename(row)"
            @keydown.esc="editing = null"
            @blur="commitRename(row)"
          />
        </span>
        <span v-else class="tree__title" :title="rowLabel(row)">{{ rowLabel(row) }}</span>

        <span v-if="row.holds_body" class="tree__words">{{ row.has_body ? formatCaliberNumber(row, caliber) : t("tree.empty_chapter") }}</span>
        <span
          v-else
          class="tree__words"
          :title="
            t('tree.volume_stat_title', {
              chapters: row.chapter_count,
              words: formatCaliberWords(
                {
                  word_count: row.subtree_word_count,
                  char_count: row.subtree_char_count,
                  chars_no_punct: row.subtree_chars_no_punct,
                },
                caliber,
              ),
            })
          "
        >
          {{ containerLabel(row, threshold, caliber) }}
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

  </aside>
</template>

<style scoped src="./directory-pane.css"></style>
