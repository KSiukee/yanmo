<script setup lang="ts">
// 设置面板：「写作行为」偏好 + 「稿子放在哪」。
//
// 视图只负责"显示与改"：偏好的真相在核心（单一真相源）；读不出来就如实说，不猜一个默认值糊上去。
// 「稿子放在哪」只显示壳报告出来的路径，另外把"换位置"那个面板请出来（它自己那套分寸见组件里）。
import { computed, ref } from "vue";

import { t } from "../locales/index.ts";
import type { EditorSession } from "../editor/session";
import LocationDialog from "./LocationDialog.vue";

const props = defineProps<{ session: EditorSession }>();
const { values, busy, close, setJumpToEnd, resetToDefault } = props.session.appearance;

/** 勾选框的当前值（读不出来就显示未勾选，并禁用） */
const jumpToEnd = computed(() => values.value?.jump_to_end_on_latest ?? false);
const unavailable = computed(() => values.value === null);

/** 稿子现在放在哪（壳报告的只读路径） */
const dataPath = computed(() => props.session.location.info.value?.path ?? "");

/** 「换位置」：先把现状读一遍再把这个面板请出来（它盖在设置上面那一层） */
const relocating = ref(false);
function startRelocate(): void {
  void props.session.location.load();
  relocating.value = true;
}

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

      <p class="settings__group settings__group--data">{{ t("settings.group_data") }}</p>
      <p class="settings__row settings__row--path">
        <span class="settings__label">{{ t("location.current") }}</span>
        <code class="settings__path">{{ dataPath }}</code>
      </p>
      <p class="settings__hint">{{ t("location.change_later") }}</p>
      <button type="button" class="settings__button dialog__button" :disabled="!dataPath" @click="startRelocate">
        {{ t("location.change") }}
      </button>

      <p v-if="unavailable" class="settings__hint settings__hint--bad">
        {{ t("settings.unavailable") }}
      </p>

      <LocationDialog v-if="relocating" :session="session" mode="settings" @close="relocating = false" />
    </section>
  </div>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./settings-dialog.css"></style>
