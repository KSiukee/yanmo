<script setup lang="ts">
// 研墨主布局：三栏骨架。
//
// ★ 侧栏位置**现在就预留**：
//   布局层**预留第二栏（创作流 / 叩问）的位置**——现在放占位，
//   等引导问答功能落地时直接填充，不用重排布局。
//
// 壳层纪律：这里只搭布局与接线，不写业务逻辑（业务在 yanmo-core）。
import { useEditorSession } from "./editor/session";
import DirectoryPane from "./components/DirectoryPane.vue";
import EditorPane from "./components/EditorPane.vue";
import FlowPane from "./components/FlowPane.vue";
import EngineBadge from "./components/EngineBadge.vue";
import SettingsDialog from "./components/SettingsDialog.vue";
import ShelfDialog from "./components/ShelfDialog.vue";
import TrashDialog from "./components/TrashDialog.vue";

// 会话在布局层建**一次**：目录树、书架与正文编辑器说的必须是同一本书、同一章
const session = useEditorSession();
const { chapterTitle, workId } = session;
const { visible: shelfVisible, toggle: toggleShelf } = session.shelf;
const { visible: trashVisible } = session.trash;
const { visible: settingsVisible, values: settingsValues, open: openSettings } = session.appearance;
</script>

<template>
  <div class="shell">
    <header class="shell__bar">
      <span class="shell__brand">研墨</span>
      <button
        type="button"
        class="shell__shelf"
        :title="workId ? '换一本书 / 新建一本书' : '打开书架'"
        @click="toggleShelf()"
      >
        书架
      </button>
      <button
        type="button"
        class="shell__settings"
        title="外观与写作行为偏好"
        @click="void openSettings()"
      >
        设置
      </button>
      <span class="shell__hint">{{ chapterTitle || "AI 只问，不写" }}</span>
      <EngineBadge />
    </header>

    <main class="shell__body">
      <DirectoryPane :session="session" />
      <EditorPane :session="session" />
      <FlowPane />
    </main>

    <ShelfDialog v-if="shelfVisible" :session="session" />
    <TrashDialog v-if="trashVisible" :session="session" />
    <SettingsDialog v-if="settingsVisible && settingsValues" :session="session" />
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
