<script setup lang="ts">
// 研墨主布局：三栏骨架。
//
// ★ 第二栏（右侧那一栏）住着两条线，用上面的页签切：
//   **叩问**（机制挑问题问你）与**创作流**（你自己记下的碎片）。
//   两块共用同一张碎片表，但界面各管各的——布局层只负责"现在露哪一块"。
//
// 壳层纪律：这里只搭布局与接线，不写业务逻辑（业务在 yanmo-core）。
import { computed, ref } from "vue";

import { t } from "./locales/index.ts";
import { useEditorSession } from "./editor/session";
import { useAsideTab } from "./editor/aside";
import { useQuestionPush } from "./editor/question-push.ts";
import type { SelectedQuestion } from "./api/question.ts";
import DirectoryPane from "./components/DirectoryPane.vue";
import EditorPane from "./components/EditorPane.vue";
import AsidePane from "./components/AsidePane.vue";
import TopBar from "./components/TopBar.vue";
import EntityDialog from "./components/EntityDialog.vue";
import BackupDialog from "./components/BackupDialog.vue";
import LocationDialog from "./components/LocationDialog.vue";
import RestoreDialog from "./components/RestoreDialog.vue";
import SettingsDialog from "./components/SettingsDialog.vue";
import ShelfDialog from "./components/ShelfDialog.vue";
import WorkFormDialog from "./components/WorkFormDialog.vue";
import SnapshotDialog from "./components/SnapshotDialog.vue";
import TrashDialog from "./components/TrashDialog.vue";
import CompileDialog from "./components/CompileDialog.vue";
import TypesetDialog from "./components/TypesetDialog.vue";
import WritingDialog from "./components/WritingDialog.vue";

// 会话在布局层建**一次**：目录树、书架与正文编辑器说的必须是同一本书、同一章
const session = useEditorSession();

// 叩问的「推」：门槛（每天几次 / 冷却）在核心算，这里只管三个时机——
// 开新章 / 卡住一会儿 / 刚写完一章（判据见 editor/question-push.ts）。
// 它读的是会话现成的状态（当前章 + 字数），不与打字那条快路耦合。
const push = useQuestionPush({
  session,
  quota: () => ({
    perDay: session.appearance.workValues.value?.question_push_per_day ?? 3,
    cooldownMinutes: session.appearance.workValues.value?.question_push_cooldown_minutes ?? 60,
  }),
  // 错过的推是静默的（配额用完 / 冷却没到 / 没得问）：不报错、不打扰写作
  onError: () => undefined,
});
/** 「答一句」点过之后，把那张卡交给右侧面板打开（面板接住后会回报一声，这里清掉） */
const pushedCard = ref<SelectedQuestion | null>(null);

// 第二栏现在露哪一块：叩问还是创作流。**记住上次**（存偏好，跟人不跟书）——
// 取与存都在 editor/aside.ts，布局层只拿"现在露哪块"和"换一块"。
const { tab: asideTab, pick: pickAside } = useAsideTab(session.appearance);
function answerPushed() {
  // 提示条上的「答一句」要落到叩问那一块：先把那一块露出来，再把卡交过去
  pickAside("flow");
  pushedCard.value = push.take();
}
const { chapterTitle, workId } = session;
const { visible: shelfVisible, toggle: toggleShelf, form: workForm, closeForm } = session.shelf;
const { visible: trashVisible } = session.trash;
const { visible: snapshotsVisible } = session.snapshots;
const { visible: typesetVisible } = session.typeset;
const { visible: compileVisible } = session.compile;
const { visible: settingsVisible, values: settingsValues, open: openSettings } = session.appearance;
// 「设定」（人物与设定 / 伏笔）：入口在顶栏；它是大纲体检的数据源，打开才读
const { visible: loreVisible, show: showLore } = session.lore;
// 专注模式：只改"露哪几块"（判断在 editor/zen.ts），布局层照着渲染，不自己 if
const { on: zenOn, chrome: zenChrome, toggle: toggleZen } = session.zen;
// 专注时的悬浮卡片：一次只开一张；卡片里放的是**原来那个目录树组件**，不写第二份
const { panels, togglePanel, closePanel } = session;
const outlineOpen = computed(() => panels.isOpen("outline"));
// 码字日历：入口在编辑器状态栏那行「今日 …」（也就是"每天手感"那一块）
const { visible: writingVisible } = session.writing;
// 备份：入口在顶栏；没有异盘目标时给一条**非阻塞**小条（拒过就不再自动弹）
const { visible: backupVisible, status: backupStatus, open: openBackup } = session.backup;
// 从备份恢复：入口在备份面板里（换库是重动作，不放在顶栏随手可点）
const { visible: restoreVisible } = session.restore;
// 稿子放在哪：第一次用才引导（壳说"这是第一次"才亮），之后要换位置去「设置」
const { visible: locationVisible, info: locationInfo } = session.location;

// 顶栏那行小字：有章名就显示章名；开着书但还没起名就显示占位；没有书才说立场那句话
const hint = computed(() => {
  if (chapterTitle.value) return chapterTitle.value;
  return workId.value ? t("common.untitled") : t("app.tagline");
});
</script>

<template>
  <div class="shell">
    <TopBar
      v-if="zenChrome.topbar"
      :work-id="workId"
      :hint="hint"
      @shelf="toggleShelf()"
      @zen="toggleZen()"
      @entities="showLore()"
      @backup="void openBackup()"
      @settings="void openSettings()"
    />

    <!-- 提醒**只有这一个出口**：一次只显示一条（软件不该抢自己的话头）。
         叩问的推优先——它跟"当下正在写的东西"直接相关；备份提醒排在后面。 -->
    <p v-if="push.tip.value" class="shell__nudge">
      <span>{{ t("flow.push.tip", { body: push.tip.value.body }) }}</span>
      <button type="button" class="shell__nudge-go" @click="answerPushed()">
        {{ t("flow.push.answer") }}
      </button>
      <button type="button" class="shell__nudge-no" @click="push.dismiss()">
        {{ t("flow.push.later") }}
      </button>
    </p>
    <p v-else-if="backupStatus?.should_nudge" class="shell__nudge">
      <span>{{ t("backup.tip") }}</span>
      <button type="button" class="shell__nudge-go" @click="void openBackup()">
        {{ t("backup.tip_open") }}
      </button>
      <button type="button" class="shell__nudge-no" @click="void session.backup.dismissTip()">
        {{ t("backup.tip_dismiss") }}
      </button>
    </p>

    <main class="shell__body" :class="{ 'shell__body--zen': zenOn }">
      <DirectoryPane v-if="zenChrome.directory" :session="session" />

      <!-- 专注时：两侧栏换成"唤出来看一眼"的卡片（同一批组件，不写第二份树）。
           ⚠️ 卡片**浮在常驻留白里**，不占列、也不挤压正文：
           正文宽度在专注态是恒定的（左边留了一条 gutter），开/关卡片只改变"显示不显示"，
           一个换行点都不会动——真机上反馈过"占列导致文字重排，观感不好"。 -->
      <template v-if="zenOn">
        <button
          type="button"
          class="shell__float"
          :title="t('zen.outline_open')"
          @click="togglePanel('outline')"
        >
          {{ t("zen.outline") }}
        </button>
        <section v-if="outlineOpen" class="shell__card">
          <header class="shell__card-bar">
            <span class="shell__card-title">{{ t("zen.outline") }}</span>
            <button
              type="button"
              class="shell__card-close"
              :title="t('zen.card_close')"
              @click="closePanel()"
            >
              {{ t("common.close") }}
            </button>
          </header>
          <DirectoryPane :session="session" />
        </section>
      </template>

      <EditorPane :session="session" />
      <!-- 第二栏（右侧）：叩问与创作流共用，顶上页签切——整块在 AsidePane 里 -->
      <AsidePane
        v-if="zenChrome.flow"
        :tab="asideTab"
        :session="session"
        :work-id="workId"
        :open-question="pushedCard"
        @update:tab="pickAside"
        @opened="pushedCard = null"
      />
    </main>

    <ShelfDialog v-if="shelfVisible" :session="session" />
    <WorkFormDialog v-if="workForm" :session="session" :form="workForm" @close="closeForm" />
    <TrashDialog v-if="trashVisible" :session="session" />
    <SnapshotDialog v-if="snapshotsVisible" :session="session" />
    <TypesetDialog v-if="typesetVisible" :session="session" />
    <WritingDialog v-if="writingVisible" :session="session" />
    <CompileDialog v-if="compileVisible" :session="session" />
    <SettingsDialog v-if="settingsVisible && settingsValues" :session="session" />
    <BackupDialog v-if="backupVisible && backupStatus" :session="session" />
    <EntityDialog v-if="loreVisible" :session="session" />
    <RestoreDialog v-if="restoreVisible" :session="session" />
    <LocationDialog
      v-if="locationVisible && locationInfo"
      :session="session"
      mode="first-run"
    />
  </div>
</template>

<style scoped>
.shell {
  display: flex;
  flex-direction: column;
  height: 100%;
}

/* 插盘提醒：**非阻塞**一行小条，点了才打开备份设置，不打断写作 */
.shell__nudge {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 0;
  padding: 6px 16px;
  border-bottom: 1px solid var(--ym-line);
  background: var(--ym-paper-dim);
  font-size: 12px;
  color: var(--ym-ink-soft);
}

.shell__nudge-go,
.shell__nudge-no {
  border: none;
  background: none;
  font: inherit;
  cursor: pointer;
  padding: 0;
}

.shell__nudge-go {
  color: var(--ym-accent);
}

.shell__nudge-no {
  color: var(--ym-ink-soft);
}

.shell__body {
  flex: 1;
  display: grid;
  grid-template-columns: 240px minmax(0, 1fr) 320px;
  /* ⚠️ 行高必须钉死在这一行里：不给 minmax(0,1fr) 的话，隐含行会按内容高度长，
     正文一长就把整页撑开、连顶栏与两侧栏一起滚走（见 layout.test.ts 的守卫）。
     overflow:hidden 是第二道保险：万一还有子元素想撑，也只是被裁在这里，不会顶开整页。 */
  grid-template-rows: minmax(0, 1fr);
  overflow: hidden;
  min-height: 0;
  /* 悬浮卡片与悬浮按钮的定位基准（它们浮在正文之上，不参与分栏） */
  position: relative;
}

/* 专注模式：只剩正文那一列；同时**常驻一条左侧留白**（卡片待的地方）。
   留白用变量给：正文本体的行宽按它收窄（见 editor-pane.css 的 --wide），
   于是开/关卡片只改"显示不显示"——正文宽度恒定，一个换行点都不动。
   窄窗口下留白跟着缩（24vw），卡片也按它收窄，免得小窗口里正文只剩一条。 */
.shell__body--zen {
  grid-template-columns: minmax(0, 1fr);
  --zen-gutter: min(324px, 24vw);
}

/* 悬浮唤出按钮：**只在专注时出现**（常规态两侧栏本来就在，用不着它）。
   ⚠️ 位置要避开编辑器那条 bar（它就在 .shell__body 的最顶上，36px 上下）——
   压在章名上既挡字又点不准，所以从 46px 起。 */
.shell__float {
  position: absolute;
  z-index: 3;
  top: 46px;
  left: 12px;
  padding: 1px 10px;
  border: 1px solid var(--ym-line);
  border-radius: 999px;
  background: var(--ym-paper);
  color: var(--ym-ink-soft);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
  box-shadow: 0 1px 4px rgb(0 0 0 / 10%);
}

.shell__float:hover {
  border-color: var(--ym-accent);
  color: var(--ym-accent);
}

/* 卡片：浮在**左边那条常驻留白**里（见 .shell__body--zen 的 --zen-gutter），
   不占列、不挤压正文——正文宽度恒定，开关卡片不会让文字重排。
   高度吃内容、最多占满整行，长了在卡片里滚（目录树自己有滚动区）。 */
.shell__card {
  position: absolute;
  z-index: 3;
  top: 74px;
  left: 12px;
  display: flex;
  flex-direction: column;
  width: calc(var(--zen-gutter) - 24px);
  max-height: calc(100% - 86px);
  border: 1px solid var(--ym-line);
  border-radius: 8px;
  background: var(--ym-paper-dim);
  box-shadow: 0 4px 16px rgb(0 0 0 / 14%);
  overflow: hidden;
}

.shell__card-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 4px 6px 4px 10px;
  border-bottom: 1px solid var(--ym-line);
}

.shell__card-title {
  font-size: 12px;
  letter-spacing: 0.1em;
  color: var(--ym-ink-soft);
}

.shell__card-close {
  padding: 1px 8px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: var(--ym-paper);
  color: inherit;
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}

.shell__card-close:hover {
  border-color: var(--ym-accent);
  color: var(--ym-accent);
}

/* 第二栏（叩问 / 创作流）整块搬进了 [`AsidePane`](./components/AsidePane.vue)：
   布局层这边只剩"露不露它"（专注模式），栏自己的边框、底色与页签都归那边。 */

/* 卡片里塞的是原来那个侧栏组件：去掉"当一列用"时的右边框与底色，让它老实待在卡里 */
.shell__card :deep(.pane) {
  border-right: none;
  background: transparent;
}
</style>
