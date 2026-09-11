<script setup lang="ts">
// 设置面板：第一版只有「写作行为」一项（外观各项会陆续加进来——它们共用这一个出口）。
//
// 视图只负责"显示与改"：偏好的真相在核心（单一真相源）；读不出来就如实说，不猜一个默认值糊上去。
import { computed } from "vue";

import type { EditorSession } from "../editor/session";

const props = defineProps<{ session: EditorSession }>();
const { values, busy, close, setJumpToEnd, resetToDefault } = props.session.appearance;

/** 勾选框的当前值（读不出来就显示未勾选，并禁用） */
const jumpToEnd = computed(() => values.value?.jump_to_end_on_latest ?? false);
const unavailable = computed(() => values.value === null);

/** 勾选框：改完写回核心，界面显示的永远是库里那份 */
function onToggle(event: Event) {
  const box = event.target as HTMLInputElement;
  void setJumpToEnd(box.checked);
}
</script>

<template>
  <div class="settings dialog" @click.self="close">
    <section class="settings__box dialog__box">
      <header class="settings__head dialog__head">
        <h2 class="settings__title dialog__title">设置</h2>
        <button
          type="button"
          class="settings__button dialog__button"
          :disabled="busy"
          title="回到核心里的默认值"
          @click="resetToDefault"
        >
          恢复默认
        </button>
        <button type="button" class="settings__button dialog__button" @click="close">关闭</button>
      </header>

      <p class="settings__group">写作行为</p>
      <label class="settings__row">
        <input
          type="checkbox"
          :checked="jumpToEnd"
          :disabled="busy || unavailable"
          @change="onToggle"
        />
        <span>打开最新一章时，跳到段末并聚焦输入光标</span>
      </label>
      <p class="settings__hint">
        历史章节默认只看不写（打开时不会把光标放进正文，免得误触插进乱字符）；点一下正文即可编辑。
      </p>
      <p class="settings__hint">这些偏好只影响显示与手感：不进导出，也不改动正文一个字。</p>
      <p v-if="unavailable" class="settings__hint settings__hint--bad">
        偏好没能读出来，这次按默认值走。
      </p>
    </section>
  </div>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./settings-dialog.css"></style>
