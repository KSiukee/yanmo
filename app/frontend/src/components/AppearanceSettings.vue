<script setup lang="ts">
// 「外观」那一栏：界面配色（浅色 / 深色 / 跟随系统）+ **用眼提醒**（开关与两个阈值）。
// 「用眼提醒」也落在这里：它跟配色同属"跟人不跟书、只走全局"的那一族设置（阈值跟哪本书无关）。
//
// 为什么单独成组件：设置面板本身已经很长（上帝棘轮盯着 400 行）；这一栏自成一体
// （一个三选一 + 一段说明），而且**将来还会长**（护眼滤色那一类就落在这里）——
// 拆出来两边都清清爽爽（与 TypographySettings 同一条分寸）。
//
// 组件只管"显示与改"：真值在核心（`appearance.theme`，跟人不跟书、只走全局），
// 落到屏幕上那一步在 `editor/theme.ts`（App.vue 接了线，改完立刻见效）。
//
// 用眼提醒那一块只在"作者要停下来的时刻"说一句（换章 / 合上稿子之前）——
// 判据与记账在 `editor/break-reminder.ts`，这里只管"显示与改"。
import { computed } from "vue";

import { t } from "../locales/index.ts";
import type { EditorSession } from "../editor/session";
import { asThemeChoice, THEME_CHOICES, type ThemeChoice } from "../editor/theme.ts";
import { BREAK_MINUTES_RANGE, BREAK_WORDS_RANGE } from "../editor/break-reminder.ts";

const props = defineProps<{ session: EditorSession }>();
const { values, busy, setTheme, setBreakReminder } = props.session.appearance;

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

// ── 用眼提醒：开关 + 两个阈值 ────────────────────────────────────
// 值从核心读回来（默认值只在核心那一处：开 / 45 分钟 / 1500 字）；它还没回话时输入框是
// 禁用的，显示的就是核心那两个默认数——与「写作行为」那一栏"读不出来就禁用"同一条分寸。
// min / max 与核心的上限同口径，真闸门在核心那边。
const breakOn = computed(() => values.value?.break_reminder ?? false);
const breakMinutes = computed(() => values.value?.break_minutes ?? 45);
const breakWords = computed(() => values.value?.break_words ?? 1500);

/** 三项一起写回去（核心那头是稀疏合并；界面上它们本来就是一件事的三个数）。 */
function writeBreak(on: boolean, minutes: number, words: number) {
  void setBreakReminder(on, minutes, words);
}

/** 数字输入：认不出来（空 / 乱写）就当没动过，别把 NaN 递给核心。 */
function numberFrom(event: Event, fallback: number): number {
  const parsed = Number.parseInt((event.target as HTMLInputElement).value, 10);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function onBreakToggle(event: Event) {
  writeBreak((event.target as HTMLInputElement).checked, breakMinutes.value, breakWords.value);
}
function onBreakMinutes(event: Event) {
  writeBreak(breakOn.value, numberFrom(event, breakMinutes.value), breakWords.value);
}
function onBreakWords(event: Event) {
  writeBreak(breakOn.value, breakMinutes.value, numberFrom(event, breakWords.value));
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
  <p class="settings__group">{{ t("settings.group_break") }}</p>
  <label class="settings__row">
    <input
      type="checkbox"
      :checked="breakOn"
      :disabled="busy || unavailable"
      @change="onBreakToggle"
    />
    <span>{{ t("settings.break_on") }}</span>
  </label>
  <label class="settings__row">
    <span>{{ t("settings.break_minutes") }}</span>
    <input
      class="settings__number"
      type="number"
      :min="BREAK_MINUTES_RANGE.min"
      :max="BREAK_MINUTES_RANGE.max"
      :value="breakMinutes"
      :disabled="busy || unavailable"
      @change="onBreakMinutes"
    />
  </label>
  <label class="settings__row">
    <span>{{ t("settings.break_words") }}</span>
    <input
      class="settings__number"
      type="number"
      :min="BREAK_WORDS_RANGE.min"
      :max="BREAK_WORDS_RANGE.max"
      :value="breakWords"
      :disabled="busy || unavailable"
      @change="onBreakWords"
    />
  </label>
  <p class="settings__hint">{{ t("settings.break_hint") }}</p>
</template>

<style scoped src="./settings-dialog.css"></style>
