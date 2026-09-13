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
import { useEditorSession } from "./editor/session";
import DirectoryPane from "./components/DirectoryPane.vue";
import EditorPane from "./components/EditorPane.vue";
import FlowPane from "./components/FlowPane.vue";
import EngineBadge from "./components/EngineBadge.vue";
import BackupDialog from "./components/BackupDialog.vue";
import RestoreDialog from "./components/RestoreDialog.vue";
import SettingsDialog from "./components/SettingsDialog.vue";
import ShelfDialog from "./components/ShelfDialog.vue";
import SnapshotDialog from "./components/SnapshotDialog.vue";
import TrashDialog from "./components/TrashDialog.vue";

// 会话在布局层建**一次**：目录树、书架与正文编辑器说的必须是同一本书、同一章
const session = useEditorSession();
const { chapterTitle, workId } = session;
const { visible: shelfVisible, toggle: toggleShelf } = session.shelf;
const { visible: trashVisible } = session.trash;
const { visible: snapshotsVisible } = session.snapshots;
const { visible: settingsVisible, values: settingsValues, open: openSettings } = session.appearance;
// 备份：入口在顶栏；没有异盘目标时给一条**非阻塞**小条（拒过就不再自动弹）
const { visible: backupVisible, status: backupStatus, open: openBackup } = session.backup;
// 从备份恢复：入口在备份面板里（换库是重动作，不放在顶栏随手可点）
const { visible: restoreVisible } = session.restore;

// 顶栏那行小字：有章名就显示章名；开着书但还没起名就显示占位；没有书才说立场那句话
const hint = computed(() => {
  if (chapterTitle.value) return chapterTitle.value;
  return workId.value ? t("common.untitled") : t("app.tagline");
});
</script>

<template>
  <div class="shell">
    <header class="shell__bar">
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

    <main class="shell__body">
      <DirectoryPane :session="session" />
      <EditorPane :session="session" />
      <FlowPane />
    </main>

    <ShelfDialog v-if="shelfVisible" :session="session" />
    <TrashDialog v-if="trashVisible" :session="session" />
    <SnapshotDialog v-if="snapshotsVisible" :session="session" />
    <SettingsDialog v-if="settingsVisible && settingsValues" :session="session" />
    <BackupDialog v-if="backupVisible && backupStatus" :session="session" />
    <RestoreDialog v-if="restoreVisible" :session="session" />
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
  min-height: 0;
}
</style>
