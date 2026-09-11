<script setup lang="ts">
// 关窗被拦下时的阻塞对话框：**存不下去就别想走**。
//
// 三个出口，都不替用户做决定：重试保存 / 导出到文件 / 仍然退出。
// 导出路径由核心给出（界面不碰文件系统），这里只负责显示与说清楚它是什么。
defineProps<{
  message: string;
  escapePath: string | null;
  busy: boolean;
}>();

const emit = defineEmits<{ retry: []; escape: []; force: [] }>();
</script>

<template>
  <div class="mask" role="alertdialog" aria-modal="true">
    <div class="card">
      <h2 class="card__title">还有内容没有存下去</h2>
      <p class="card__message">{{ message }}</p>
      <p class="card__lead">这一章先别急着关。可以重试保存，也可以把手上这份导出成文件带走。</p>

      <p v-if="escapePath" class="card__path">
        已导出到：<code>{{ escapePath }}</code>
      </p>

      <div class="card__actions">
        <button type="button" :disabled="busy" @click="emit('retry')">重试保存</button>
        <button type="button" :disabled="busy" @click="emit('escape')">导出到文件</button>
        <button type="button" class="card__danger" :disabled="busy" @click="emit('force')">
          仍然退出
        </button>
      </div>

      <p class="card__note">
        选「仍然退出」不会删除任何东西：已经存进库里的字都在；只是这一次没能确认落盘的内容会丢。
      </p>
    </div>
  </div>
</template>

<style scoped>
.mask {
  position: fixed;
  inset: 0;
  display: grid;
  place-items: center;
  background: rgba(24, 20, 16, 0.45);
  z-index: 20;
}

.card {
  width: min(32em, 90vw);
  padding: 20px 22px 16px;
  border: 1px solid var(--ym-line);
  border-radius: 10px;
  background: var(--ym-paper);
  box-shadow: 0 18px 48px rgba(0, 0, 0, 0.28);
  font-size: 13px;
  line-height: 1.7;
}

.card__title {
  margin: 0 0 8px;
  font-size: 15px;
  font-weight: 600;
}

.card__message {
  margin: 0 0 6px;
  color: #b3261e;
}

.card__lead {
  margin: 0 0 12px;
  color: var(--ym-ink-soft);
}

.card__path {
  margin: 0 0 12px;
  padding: 8px 10px;
  border-radius: 6px;
  background: var(--ym-paper-dim);
  word-break: break-all;
}

.card__path code {
  font-family: var(--ym-font-mono, monospace);
  font-size: 12px;
}

.card__actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
}

.card__actions button {
  padding: 6px 14px;
  border: 1px solid var(--ym-line);
  border-radius: 6px;
  background: var(--ym-paper);
  color: inherit;
  font: inherit;
  cursor: pointer;
}

.card__actions button:disabled {
  opacity: 0.5;
  cursor: default;
}

.card__danger {
  color: #b3261e;
}

.card__note {
  margin: 10px 0 0;
  font-size: 12px;
  color: var(--ym-ink-soft);
}
</style>
