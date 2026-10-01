<script setup lang="ts">
// 拆章的确认层：**改正文的动作，先把这一刀切在哪儿摆给作者看，确认了才动**。
//
// 单独一个组件有两个理由（与「恢复默认」那一层同一条）：
// 1. 编辑器面板本身别长胖（它已经在 300 行上下）；
// 2. 边界清楚：这里只管"摆清楚 + 问一句"，真正动刀的是父组件。
//
// 三件事必须摆出来（这条任务的验收写的就是它们）：
// - **切在哪儿**：前面几段留在这一章、后面几段进新章（段数是界面自己算的，如实报）；
// - **新章叫什么**：留空就由核心按同号取（作者随时能改）；
// - **怎么退回去**：拆完条上会给一次「撤销」，底也留在版本历史里。
import { ref } from "vue";

import { t } from "../locales/index.ts";
import type { SplitPoint } from "../editor/split.ts";

const props = defineProps<{ split: SplitPoint; busy: boolean }>();
const emit = defineEmits<{ cancel: []; confirm: [title: string] }>();

const title = ref("");

function confirm() {
  emit("confirm", title.value.trim());
}
</script>

<template>
  <div class="split dialog dialog--above" @click.self="emit('cancel')">
    <section class="split__box dialog__box">
      <header class="split__head dialog__head">
        <h2 class="split__title dialog__title">{{ t("split.title") }}</h2>
      </header>
      <p class="split__hint">
        {{ t("split.where", { kept: props.split.kept, moved: props.split.moved }) }}
      </p>
      <label class="split__field">
        <span class="split__label">{{ t("split.name_label") }}</span>
        <input
          v-model="title"
          class="split__input"
          type="text"
          :placeholder="t('split.name_placeholder')"
          @keyup.enter="confirm()"
        />
      </label>
      <p class="split__hint dialog__note">{{ t("split.undo_hint") }}</p>
      <div class="split__actions">
        <button type="button" class="split__button dialog__button" @click="emit('cancel')">
          {{ t("common.cancel") }}
        </button>
        <button
          type="button"
          class="split__button dialog__button split__button--go"
          :disabled="busy"
          @click="confirm()"
        >
          {{ t("split.ok") }}
        </button>
      </div>
    </section>
  </div>
</template>

<style scoped src="./dialog.css"></style>
<style scoped>
.split__box {
  width: min(460px, 100%);
}

.split__hint {
  margin: 0 0 10px;
  font-size: 12px;
  line-height: 1.6;
}

.split__field {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 10px;
  font-size: 12px;
}

.split__label {
  white-space: nowrap;
  opacity: 0.8;
}

.split__input {
  flex: 1;
  padding: 3px 8px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: var(--ym-paper);
  color: inherit;
  font: inherit;
  font-size: 12px;
}

.split__actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
