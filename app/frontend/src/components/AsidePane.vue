<script setup lang="ts">
// 右侧那一栏（第二栏）：三条线共用这一个位置，顶上三片页签切。
//
// **叩问**是"机制挑问题问你"；**创作流**是"你自己记下的碎片"；**流程**是你自己的写作流程。
// 三块住同一栏是因为它们本来就是同一件事的几面（都围着"这本书现在要什么、我下一步干什么"转），
// 而且叩问与创作流共用同一张碎片表——分成几个栏反而要把窗口挤成好几列。
//
// 这个文件只管"露哪一块"：业务分别在 [`useFlowPanel`](./flow-panel.ts) 与
// [`useCreatorPanel`](./creator-panel.ts) 里；栏的边框与底色也归这里（组件里只管内容，
// 切页签时不会因为两个组件的边框写法不同而抖一下）。
//
// 页签是**受控**的（`tab` 由布局层给）：提示条上那个「答一句」要能把叩问那一块露出来，
// 那个动作在布局层，所以"现在露哪一块"得由它说了算。
import { parseAsideTab, type AsideTab } from "../editor/appearance.ts";
import { t } from "../locales/index.ts";
import type { SelectedQuestion } from "../api/question.ts";
import type { EditorSession } from "../editor/session.ts";
import CreatorPane from "./CreatorPane.vue";
import FlowPane from "./FlowPane.vue";
import SopPane from "./SopPane.vue";

/** 流程那一片绑的动作用的是**稳定码字符串**：在这儿过同一个解析口（认不出就什么都不做）。 */
function openTab(code: string) {
  const known = parseAsideTab(code);
  if (known) emit("open-tab", known);
}

defineProps<{
  session: EditorSession;
  workId: number | null;
  tab: AsideTab;
  /** 推过来的那张卡（作者点了提示条上的「答一句」） */
  openQuestion?: SelectedQuestion | null;
}>();

const emit = defineEmits<{
  "update:tab": [AsideTab];
  opened: [];
  /** 流程那一片绑了动作的步骤要"去某块面板"——交给布局层切（与页签同一个出口） */
  "open-tab": [AsideTab];
}>();
</script>

<template>
  <aside class="aside">
    <nav class="aside__tabs">
      <button
        type="button"
        class="aside__tab"
        :class="{ 'aside__tab--on': tab === 'flow' }"
        @click="emit('update:tab', 'flow')"
      >
        {{ t("flow.title") }}
      </button>
      <button
        type="button"
        class="aside__tab"
        :class="{ 'aside__tab--on': tab === 'creator' }"
        @click="emit('update:tab', 'creator')"
      >
        {{ t("creator.tab") }}
      </button>
      <button
        type="button"
        class="aside__tab"
        :class="{ 'aside__tab--on': tab === 'sop' }"
        @click="emit('update:tab', 'sop')"
      >
        {{ t("sop.tab") }}
      </button>
    </nav>
    <FlowPane
      v-if="tab === 'flow'"
      :session="session"
      :work-id="workId"
      :open-question="openQuestion"
      @opened="emit('opened')"
    />
    <!-- 创作流那一栏的各片自己从会话里取数据（壳只管切哪一片），所以不必再传 workId -->
    <CreatorPane v-else-if="tab === 'creator'" :session="session" />
    <!-- 流程：状态就地建（改一下就存一下），所以只把"现在开着哪本书"递进去 -->
    <SopPane v-else :work-id="workId" @open-tab="openTab" />
  </aside>
</template>

<style scoped>
.aside {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  border-left: 1px solid var(--ym-line);
  background: var(--ym-paper-dim);
}

.aside__tabs {
  display: flex;
  gap: 4px;
  padding: 6px 8px 0;
}

.aside__tab {
  padding: 2px 10px;
  border: 1px solid var(--ym-line);
  border-bottom: none;
  border-radius: 6px 6px 0 0;
  background: transparent;
  color: var(--ym-ink-soft);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}

.aside__tab--on {
  background: var(--ym-paper);
  border-color: var(--ym-accent);
  color: var(--ym-accent);
}

/* 栏里那一块吃掉剩下的高度并自己滚（两块组件的 `.pane` 都是这个约定） */
.aside :deep(.pane) {
  flex: 1;
  min-height: 0;
  border-left: none;
  background: transparent;
}
</style>
