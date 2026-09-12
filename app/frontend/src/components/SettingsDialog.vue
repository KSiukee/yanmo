<script setup lang="ts">
// 设置面板：第一版只有「写作行为」一项（外观各项会陆续加进来——它们共用这一个出口）。
//
// 视图只负责"显示与改"：偏好的真相在核心（单一真相源）；读不出来就如实说，不猜一个默认值糊上去。
import { computed } from "vue";

import { t } from "../locales/index.ts";
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
        <h2 class="settings__title dialog__title">{{ t("settings.title") }}</h2>
        <button
          type="button"
          class="settings__button dialog__button"
          :disabled="busy"
          :title="t('settings.reset_title')"
          @click="resetToDefault"
        >
          {{ t("settings.reset") }}
        </button>
        <button type="button" class="settings__button dialog__button" @click="close">{{ t("common.close") }}</button>
      </header>

      <p class="settings__group">{{ t("settings.group_writing") }}</p>
      <label class="settings__row">
        <input
          type="checkbox"
          :checked="jumpToEnd"
          :disabled="busy || unavailable"
          @change="onToggle"
        />
        <span>{{ t("settings.jump_to_end") }}</span>
      </label>
      <p class="settings__hint">
        {{ t("settings.read_only_hint") }}
      </p>
      <p class="settings__hint">{{ t("settings.local_only_hint") }}</p>
      <p v-if="unavailable" class="settings__hint settings__hint--bad">
        {{ t("settings.unavailable") }}
      </p>
    </section>
  </div>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./settings-dialog.css"></style>
