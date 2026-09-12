<script setup lang="ts">
// 创作流 / 引导问答面板：**预留位**。
//
// 这里最终会长出「叩问」——从故事进展、大纲、未回收伏笔、灵感碎片等要素出发
// 实时问答引导作者。
//
// ⚠️ 纪律：AI 只问不写，面板里永远不出现 AI 生成的正文句子。

import { t } from "../locales/index.ts";

/** 计划中的能力——显示的是字典里的句子，这里只登记键 */
const planned = [
  "flow.planned.problem_cards",
  "flow.planned.topic_engine",
  "flow.planned.defer_and_park",
  "flow.planned.two_channels",
];
</script>

<template>
  <aside class="pane">
    <h2 class="pane__title">{{ t("flow.title") }}</h2>
    <p class="pane__todo">{{ t("flow.todo") }}</p>
    <ul class="pane__list">
      <li v-for="item in planned" :key="item">{{ t(item) }}</li>
    </ul>
  </aside>
</template>

<style scoped>
.pane {
  padding: 12px;
  border-left: 1px solid var(--ym-line);
  background: var(--ym-paper-dim);
  overflow: auto;
}

.pane__title {
  margin: 0 0 8px;
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.1em;
  color: var(--ym-ink-soft);
}

.pane__todo {
  margin: 0 0 10px;
  font-size: 12px;
  color: var(--ym-ink-soft);
}

.pane__list {
  margin: 0;
  padding-left: 18px;
  font-size: 12px;
  color: var(--ym-ink-soft);
  line-height: 1.9;
}
</style>
