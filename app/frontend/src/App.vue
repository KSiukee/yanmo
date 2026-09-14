<script setup lang="ts">
// 研墨主布局：三栏骨架。
//
// ★ 侧栏位置**现在就预留**：
//   布局层**预留第二栏（创作流 / 叩问）的位置**——现在放占位，
//   等引导问答功能落地时直接填充，不用重排布局。
//
// 壳层纪律：这里只搭布局与接线，不写业务逻辑（业务在 yanmo-core）。
import { computed } from "vue";

import { t } from "./locales/index.ts";
import { shortcutKeys } from "./editor/shortcuts.ts";
import { useEditorSession } from "./editor/session";
import DirectoryPane from "./components/DirectoryPane.vue";
import EditorPane from "./components/EditorPane.vue";
import FlowPane from "./components/FlowPane.vue";
import EngineBadge from "./components/EngineBadge.vue";
import BackupDialog from "./components/BackupDialog.vue";
import LocationDialog from "./components/LocationDialog.vue";
import RestoreDialog from "./components/RestoreDialog.vue";
import SettingsDialog from "./components/SettingsDialog.vue";
import ShelfDialog from "./components/ShelfDialog.vue";
import SnapshotDialog from "./components/SnapshotDialog.vue";
import TrashDialog from "./components/TrashDialog.vue";
import CompileDialog from "./components/CompileDialog.vue";
import TypesetDialog from "./components/TypesetDialog.vue";
import WritingDialog from "./components/WritingDialog.vue";

// 会话在布局层建**一次**：目录树、书架与正文编辑器说的必须是同一本书、同一章
const session = useEditorSession();
const { chapterTitle, workId } = session;
const { visible: shelfVisible, toggle: toggleShelf } = session.shelf;
const { visible: trashVisible } = session.trash;
const { visible: snapshotsVisible } = session.snapshots;
const { visible: typesetVisible } = session.typeset;
const { visible: compileVisible } = session.compile;
const { visible: settingsVisible, values: settingsValues, open: openSettings } = session.appearance;
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
    <header v-if="zenChrome.topbar" class="shell__bar">
      <!-- i18n-allow-next-line: 产品名（品牌），不是界面文案 -->
      <span class="shell__brand">研墨</span>
      <button
        type="button"
        class="shell__shelf"
        :title="workId ? t('app.shelf_title_switch') : t('app.shelf_title_open')"
        @click="toggleShelf()"
      >
        {{ t("app.shelf") }}
      </button>
      <button
        type="button"
        class="shell__settings"
        :title="t('app.zen_title', { key: shortcutKeys('zen') })"
        @click="toggleZen()"
      >
        {{ t("app.zen") }}
      </button>
      <button
        type="button"
        class="shell__settings"
        :title="t('app.settings_title')"
        @click="void openSettings()"
      >
        {{ t("app.settings") }}
      </button>
      <button
        type="button"
        class="shell__settings"
        :title="t('app.backup_title')"
        @click="void openBackup()"
      >
        {{ t("app.backup") }}
      </button>
      <span class="shell__hint">{{ hint }}</span>
      <EngineBadge />
    </header>

    <p v-if="backupStatus?.should_nudge" class="shell__nudge">
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
      <EditorPane :session="session" />
      <FlowPane v-if="zenChrome.flow" />

      <!-- 专注时：两侧栏换成"唤出来看一眼"的悬浮卡片（同一批组件，不写第二份树） -->
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
    </main>

    <ShelfDialog v-if="shelfVisible" :session="session" />
    <TrashDialog v-if="trashVisible" :session="session" />
    <SnapshotDialog v-if="snapshotsVisible" :session="session" />
    <TypesetDialog v-if="typesetVisible" :session="session" />
    <WritingDialog v-if="writingVisible" :session="session" />
    <CompileDialog v-if="compileVisible" :session="session" />
    <SettingsDialog v-if="settingsVisible && settingsValues" :session="session" />
    <BackupDialog v-if="backupVisible && backupStatus" :session="session" />
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

.shell__bar {
  display: flex;
  align-items: baseline;
  gap: 12px;
  padding: 10px 16px;
  border-bottom: 1px solid var(--ym-line);
  background: var(--ym-paper-dim);
}

.shell__brand {
  font-weight: 600;
  letter-spacing: 0.08em;
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

.shell__hint {
  font-size: 12px;
  color: var(--ym-ink-soft);
}

.shell__shelf,
.shell__settings {
  padding: 1px 10px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: var(--ym-paper);
  color: inherit;
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}

.shell__shelf:hover,
.shell__settings:hover {
  border-color: var(--ym-accent);
  color: var(--ym-accent);
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

/* 专注模式：只剩正文那一列（两侧栏没渲染，这里也就没有第二、三列可占） */
.shell__body--zen {
  grid-template-columns: minmax(0, 1fr);
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

/* 卡片：浮在正文之上**不挤正文**；收起即消失（写作时不该被布局变化打断）。
   水平方向落在正文左侧的留白里（专注时正文是居中的，左右各有余量）。 */
.shell__card {
  position: absolute;
  z-index: 3;
  top: 74px;
  left: 12px;
  display: flex;
  flex-direction: column;
  width: 300px;
  max-height: calc(100% - 86px);
  border: 1px solid var(--ym-line);
  border-radius: 8px;
  background: var(--ym-paper-dim);
  box-shadow: 0 8px 28px rgb(0 0 0 / 18%);
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

/* 卡片里塞的是原来那个侧栏组件：去掉"当一列用"时的右边框与底色，让它老实待在卡里 */
.shell__card :deep(.pane) {
  border-right: none;
  background: transparent;
}
</style>
