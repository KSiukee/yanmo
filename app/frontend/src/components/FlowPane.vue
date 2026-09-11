<script setup lang="ts">
// 创作流 / 引导问答面板：**预留位**。
//
// 这里最终会长出「叩问」——从故事进展、大纲、未回收伏笔、灵感碎片等要素出发
// 实时问答引导作者。
//
// ⚠️ 纪律：AI 只问不写，面板里永远不出现 AI 生成的正文句子。

const planned = ["问题卡与状态机", "加权选题引擎", "延后 / 舍弃 / 记灵感", "键盘与语音双通道作答"];
</script>

<template>
  <aside class="pane">
    <h2 class="pane__title">叩问</h2>
    <p class="pane__todo">引导问答待实现（P1）</p>
    <ul class="pane__list">
      <li v-for="item in planned" :key="item">{{ item }}</li>
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
