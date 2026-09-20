<script setup lang="ts">
// 「外观」那一栏：界面配色 —— 浅色 / 深色 / 跟随系统。
//
// 为什么单独成组件：设置面板本身已经很长（上帝棘轮盯着 400 行）；这一栏自成一体
// （一个三选一 + 一段说明），而且**将来还会长**（护眼滤色那一类就落在这里）——
// 拆出来两边都清清爽爽（与 TypographySettings 同一条分寸）。
//
// 组件只管"显示与改"：真值在核心（`appearance.theme`，跟人不跟书、只走全局），
// 落到屏幕上那一步在 `editor/theme.ts`（App.vue 接了线，改完立刻见效）。
import { computed } from "vue";

import { t } from "../locales/index.ts";
import type { EditorSession } from "../editor/session";
import { asThemeChoice, THEME_CHOICES, type ThemeChoice } from "../editor/theme.ts";

const props = defineProps<{ session: EditorSession }>();
const { values, busy, setTheme } = props.session.appearance;

/** 现在选的是哪一档（读不出来就显示默认那一档，并禁用） */
const current = computed(() => asThemeChoice(values.value));
const unavailable = computed(() => values.value === null);

/** 三档的说明文案键（顺序 = THEME_CHOICES） */
const LABELS: Record<ThemeChoice, string> = {
  light: "settings.theme.light",
  dark: "settings.theme.dark",
  system: "settings.theme.system",
};

function onPick(event: Event) {
  const code = (event.target as HTMLInputElement).value;
  void setTheme(code);
}
</script>

<template>
  <p class="settings__group">{{ t("settings.group_appearance") }}</p>
  <p class="settings__row">
    <span class="settings__label">{{ t("settings.theme") }}</span>
    <span class="settings__choices">
      <label v-for="choice in THEME_CHOICES" :key="choice" class="settings__choice">
        <input
          type="radio"
          name="ym-theme"
          :value="choice"
          :checked="current === choice"
          :disabled="busy || unavailable"
          @change="onPick"
        />
        <span>{{ t(LABELS[choice]) }}</span>
      </label>
    </span>
  </p>
  <p class="settings__hint">{{ t("settings.theme_hint") }}</p>
</template>

<style scoped src="./settings-dialog.css"></style>
