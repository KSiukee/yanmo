<script setup lang="ts">
// 事件的**故事时间**那一小张表单：自由文本（给人看）+ 排序值（给排序用）+ 倒叙标记。
//
// 单独成件的原因与 `RoundTray` 那些一样：它是"改这一条事件"的一整件事，
// 而碎片池那一份只管把列表摆出来——两块的变化理由不一样。
//
// 三句要说清的话：
// - **填得少就报得少**：只有"两边都填了数字、都不是倒叙"才判得动顺序；
// - **倒叙是作者自己的写法**（后写的章讲更早的事），勾上就不参与顺序检查；
// - 正文不在这一版编辑（它是**素材**，改它去正文里改）——这里只改时间与那三样。
import type { EditorSession } from "../editor/session.ts";
import type { FragmentDraft } from "./creator-panel.ts";
import { t } from "../locales/index.ts";

const props = defineProps<{
  session: EditorSession;
  /** 正在改的那一条（父层拿着；这里直接绑它上面的三栏） */
  editing: FragmentDraft;
  busy: boolean;
}>();
const emit = defineEmits<{ save: []; cancel: [] }>();
</script>

<template>
  <form class="edit" @submit.prevent="emit('save')">
    <label class="edit__field">
      <span class="edit__label">{{ t("creator.edit_body") }}</span>
      <input v-model="editing.body" class="edit__input" type="text" :disabled="busy" />
    </label>
    <label class="edit__field">
      <span class="edit__label">{{ t("creator.story_time") }}</span>
      <input
        v-model="editing.storyTime"
        class="edit__input"
        type="text"
        :placeholder="t('creator.story_time_placeholder')"
        :disabled="busy"
      />
    </label>
    <label class="edit__field">
      <span class="edit__label">{{ t("creator.story_order") }}</span>
      <input
        v-model="editing.storyOrderText"
        class="edit__input"
        type="number"
        :placeholder="t('creator.story_order_placeholder')"
        :disabled="busy"
      />
    </label>
    <label class="edit__check">
      <input v-model="editing.flashback" type="checkbox" :disabled="busy" />
      <span>{{ t("creator.flashback") }}</span>
    </label>
    <div class="edit__actions">
      <button type="submit" class="edit__button" :disabled="busy || !editing.body.trim()">
        {{ t("creator.edit_save") }}
      </button>
      <button type="button" class="edit__button" :disabled="busy" @click="emit('cancel')">
        {{ t("creator.edit_cancel") }}
      </button>
    </div>
  </form>
</template>

<style scoped>
/* 事件的故事时间：一小张表单，挤在对应的那一条底下 */
.edit {
  display: grid;
  gap: 4px;
  margin: 6px 0 2px;
  padding: 6px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: var(--ym-paper);
}

.edit__actions {
  display: flex;
  gap: 6px;
  justify-content: flex-end;
}

.edit__button {
  padding: 1px 8px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: var(--ym-paper);
  color: inherit;
  font: inherit;
  font-size: 11px;
  cursor: pointer;
}

.edit__button:disabled {
  opacity: 0.5;
  cursor: default;
}

.edit__field {
  display: grid;
  grid-template-columns: 6.5em minmax(0, 1fr);
  gap: 6px;
  align-items: center;
}

.edit__label {
  font-size: 11px;
  opacity: 0.75;
}

.edit__input {
  box-sizing: border-box;
  width: 100%;
  min-width: 0;
  padding: 2px 6px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: var(--ym-paper);
  color: inherit;
  font: inherit;
  font-size: 12px;
}

.edit__check {
  display: flex;
  gap: 6px;
  align-items: center;
  font-size: 11px;
  opacity: 0.85;
}

</style>
