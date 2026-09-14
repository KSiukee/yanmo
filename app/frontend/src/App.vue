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
}

/* 专注模式：只剩正文那一列（两侧栏没渲染，这里也就没有第二、三列可占） */
.shell__body--zen {
  grid-template-columns: minmax(0, 1fr);
}
</style>
