<script setup lang="ts">
// 创作流面板：**你自己记，研墨只替你归位**。
//
// 与叩问那条线的分工（两块都住在右侧这一栏）：叩问是"机制挑问题问你"，
// 这里是"你自己记下的东西"。它们共用**同一张碎片表**（所以灵感、答案、事件
// 在存储上是一家人），但界面各管各的：这里不建问题卡、也不处置问题。
//
// 这个文件只管**长什么样**：记什么、读回来什么、怎么删与撤销，全在
// [`useCreatorPanel`](./creator-panel.ts) 里——两份的变化理由不一样。
import { ref } from "vue";

import type { EditorSession } from "../editor/session.ts";
import { formatWhen } from "../editor/display.ts";
import { t } from "../locales/index.ts";
import {
  anchoredToChapter,
  canCarryStoryTime,
  kindLabel,
  sourceBadge,
  storyLabel,
  WRITABLE_KINDS,
} from "./creator.ts";
import EventEdit from "./EventEdit.vue";
import OutlineCheck from "./OutlineCheck.vue";

const props = defineProps<{
  workId: number | null;
  session: EditorSession;
}>();

// 传整个 `props`（不是拆出来的 workId）：组合式里那个 `watch` 要看得见"换书"
const {
  board,
  busy,
  errorText,
  filter,
  draftKind,
  justSaved,
  lastDeleted,
  editing,
  options,
  list,
  canJot,
  currentChapter,
  jot,
  startEdit,
  cancelEdit,
  saveEdit,
  remove,
  undo,
} = props.session.creator;

const draft = ref("");

async function save() {
  if (await jot(draft.value)) draft.value = "";
}
</script>

<template>
  <aside class="pane">
    <h2 class="pane__title">{{ t("creator.title") }}</h2>
    <p class="pane__rule">{{ t("creator.rule") }}</p>

    <p v-if="errorText" class="pane__error">{{ errorText }}</p>

    <!-- 记一条：**记下来不打断手上的事**——写完这一条就回到原来那张纸上 -->
    <section v-if="canJot" class="jot">
      <h3 class="sec">{{ t("creator.jot.title") }}</h3>
      <textarea
        v-model="draft"
        class="jot__box"
        rows="2"
        :placeholder="t('creator.jot.placeholder')"
        :disabled="busy"
        @keydown.ctrl.enter.prevent="save"
      ></textarea>
      <div class="row">
        <label class="jot__kind">
          <span class="hint">{{ t("creator.jot.kind") }}</span>
          <select v-model="draftKind" :disabled="busy">
            <option v-for="kind in WRITABLE_KINDS" :key="kind" :value="kind">
              {{ kindLabel(kind) }}
            </option>
          </select>
        </label>
        <button class="link" :disabled="busy || !draft.trim()" @click="save">
          {{ t("creator.jot.save") }}
        </button>
      </div>
      <p class="hint">{{ t("creator.jot.hint") }}</p>
    </section>
    <p v-else class="pane__hint">{{ t("creator.no_work") }}</p>

    <!-- 回执与那一手撤销：删是软删，所以"撤销"当场就能兑现 -->
    <section v-if="justSaved || lastDeleted !== null" class="receipt">
      <p v-if="justSaved" class="saved">{{ justSaved }}</p>
      <p v-if="lastDeleted !== null" class="saved">
        {{ t("creator.deleted") }}
        <button class="link" :disabled="busy" @click="undo">{{ t("creator.undo") }}</button>
      </p>
    </section>

    <!-- 筛选项：数字来自核心按种类算的计数（不是数当前这一屏） -->
    <nav v-if="canJot && board" class="filters">
      <button
        v-for="option in options"
        :key="option.kind"
        type="button"
        class="chip"
        :class="{ 'chip--on': filter === option.kind }"
        :disabled="busy"
        @click="filter = option.kind"
      >
        {{
          option.kind === "all"
            ? t("creator.filter.all")
            : kindLabel(option.kind)
        }}
        <span class="chip__n">{{ t("creator.count", { count: option.count }) }}</span>
      </button>
    </nav>

    <p v-if="busy && !board" class="pane__hint">{{ t("creator.loading") }}</p>
    <ul v-else-if="list.length > 0" class="frags">
      <li v-for="item in list" :key="item.id" class="frag">
        <p class="frag__head">
          <span class="frag__kind">{{ kindLabel(item.kind) }}</span>
          <span v-if="anchoredToChapter(item.anchors, currentChapter)" class="badge">
            {{ t("creator.badge.this_chapter") }}
          </span>
          <span v-if="item.derived_from !== null" class="badge">
            {{ t("creator.badge.from_question") }}
          </span>
          <span v-if="sourceBadge(item.source)" class="badge">
            {{ sourceBadge(item.source) }}
          </span>
          <span v-if="storyLabel(item)" class="badge">{{ storyLabel(item) }}</span>
          <span v-if="item.flashback" class="badge">{{ t("creator.badge.flashback") }}</span>
          <span class="frag__when">{{ formatWhen(item.created_at) }}</span>
          <button
            v-if="canCarryStoryTime(item.kind)"
            class="link"
            :disabled="busy"
            @click="startEdit(item)"
          >
            {{ t("creator.edit") }}
          </button>
          <button class="link" :disabled="busy" @click="remove(item.id)">
            {{ t("creator.action.delete") }}
          </button>
        </p>
        <p class="frag__body">{{ item.body }}</p>

        <!-- 事件才有的一小张表单：故事时间 + 倒叙（单独成件，见 EventEdit.vue） -->
        <EventEdit
          v-if="editing && editing.id === item.id"
          :session="session"
          :editing="editing"
          :busy="busy"
          @save="void saveEdit()"
          @cancel="cancelEdit()"
        />
      </li>
    </ul>
    <p v-else-if="board && !busy" class="pane__hint">{{ t("creator.empty") }}</p>

    <!-- 大纲体检：**只看不说**——对不上的地方列在这儿，改不改由作者；
         点「去设定卡改」把那一屏打开（两块住同一栏，所以不必再开一栏） -->
    <OutlineCheck
      :session="session"
      @open-entities="(tab) => session.lore.show(tab)"
      @open-foreshadows="session.lore.show('foreshadows')"
    />
  </aside>
</template>

<style scoped>
.pane {
  display: flex;
  flex-direction: column;
  min-height: 0;
  padding: 12px;
  overflow: auto;
  font-size: 13px;
}

.pane__title {
  margin: 0 0 4px;
  font-size: 13px;
}

.pane__rule,
.pane__hint,
.hint {
  margin: 0 0 8px;
  font-size: 11px;
  line-height: 1.5;
  opacity: 0.7;
}

.pane__error {
  margin: 0 0 8px;
  font-size: 11px;
  color: var(--ym-danger, #c0392b);
}

.sec {
  margin: 0 0 6px;
  font-size: 11px;
  opacity: 0.75;
}

.jot {
  margin: 0 0 8px;
}

.jot__box {
  box-sizing: border-box;
  width: 100%;
  margin: 0 0 6px;
  padding: 4px 6px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: var(--ym-paper);
  color: inherit;
  font: inherit;
  resize: vertical;
}

.jot__kind {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
}

.jot__kind .hint {
  margin: 0;
}

.row {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
  margin: 0 0 4px;
}

.link {
  padding: 2px 8px;
  font-size: 11px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: transparent;
  color: inherit;
  cursor: pointer;
}

.link:disabled {
  opacity: 0.5;
  cursor: default;
}

.receipt {
  margin: 0 0 8px;
}

.saved {
  margin: 0 0 4px;
  font-size: 11px;
  opacity: 0.8;
}

.filters {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  margin: 0 0 8px;
}

.chip {
  padding: 2px 8px;
  border: 1px solid var(--ym-line);
  border-radius: 999px;
  background: transparent;
  color: inherit;
  font: inherit;
  font-size: 11px;
  cursor: pointer;
}

.chip--on {
  border-color: var(--ym-accent);
  color: var(--ym-accent);
}

.chip__n {
  margin-left: 4px;
  opacity: 0.7;
}

.frags {
  margin: 0;
  padding: 0;
  list-style: none;
}

.frag {
  margin: 0 0 8px;
  padding: 0 0 8px;
  border-bottom: 1px solid var(--ym-line);
}

.frag__head {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  align-items: center;
  margin: 0 0 2px;
  font-size: 11px;
  opacity: 0.8;
}

.frag__kind {
  font-weight: 600;
  opacity: 1;
}

.badge {
  padding: 0 4px;
  border: 1px solid var(--ym-line);
  border-radius: 3px;
  font-size: 10px;
}

.frag__when {
  margin-left: auto;
}

.frag__body {
  margin: 0;
  font-size: 12px;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-word;
}

</style>
