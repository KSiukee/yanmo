<script setup lang="ts">
// 正文排版那一栏：**字号 / 行距 / 字距**（显示层，只改观感，不进导出）。
//
// 为什么单独成组件：设置面板本身已经很长（上帝棘轮盯着 400 行），而这一栏自成一体——
// 三个旋钮 + 一段示例字 + 恢复默认，再加下面那组「稿纸横线」，与别的设置项没有耦合，
// 拆出来两边都清清爽爽（横线跟排版是同一份偏好的两面：一格多高由行距算出来）。
// 档位、范围与换算**一律向 editor/typography.ts 与 editor/rule.ts 要**（组件里不写死 px）。
import { computed, ref } from "vue";

import { systemFonts } from "../api/core.ts";
import { t } from "../locales/index.ts";
import type { EditorSession } from "../editor/session";
import {
  asTypography,
  TYPOGRAPHY_KNOBS,
  typographyStyle,
  type TypographyField,
  type TypographyKnob,
} from "../editor/typography.ts";
import { asRuleStyle, RULE_STYLES, type RuleStyle } from "../editor/rule.ts";
import {
  asEditorFont,
  FONT_PRESETS,
  fontFamilyName,
  fontStackOf,
} from "../editor/fonts.ts";

const props = defineProps<{ session: EditorSession }>();
const { values, workValues, busy, setTypography, setRuleStyle, setEditorFont } =
  props.session.appearance;

/** 现在用的是哪一档：这本书的覆盖优先，没打开书看全局（与编辑区绑的是同一份） */
const typography = computed(() => asTypography(workValues.value ?? values.value));
const unavailable = computed(() => values.value === null);

/** 拖动旋钮就地生效：写完核心回读，编辑区跟着变（只改观感，不碰正文一个字） */
function onTypography(field: TypographyField, event: Event) {
  const value = Number.parseInt((event.target as HTMLInputElement).value, 10);
  if (Number.isFinite(value)) void setTypography(field, value);
}

/** 恢复默认那档（发 0 = 清掉这一项，核心当"没设过"处理） */
function resetTypography() {
  for (const knob of TYPOGRAPHY_KNOBS) void setTypography(knob.field, 0);
}

/** 某个旋钮现在指到哪儿（与库里那份一致；脱钩了就会"拖了弹回去"） */
function typographyValue(field: TypographyField): number {
  if (field === "editor_font_size") return typography.value.size;
  if (field === "editor_line_height") return typography.value.line;
  return typography.value.spacing;
}

/** 旋钮右边那个数：怎么显示由档位表说了算 */
function typographyLabel(knob: TypographyKnob): string {
  return knob.format(typographyValue(knob.field));
}

/** 示例那一段绑的是同一份变量（与编辑区一字不差地同源） */
const sampleStyle = computed(() => ({
  ...typographyStyle(workValues.value ?? values.value),
  fontFamily: fontStackOf(fontValue.value),
}));

// ── 正文字体：五档预设 + 系统里装了的（可搜） ──────────────────────
const fontValue = computed(() => asEditorFont(workValues.value ?? values.value));
const customFamily = computed(() => fontFamilyName(fontValue.value));
const fontsOpen = ref(false);
const fonts = ref<string[]>([]);
const fontsLoading = ref(false);
const fontsLoaded = ref(false);
const fontsError = ref("");
const fontQuery = ref("");

const filteredFonts = computed(() => {
  const query = fontQuery.value.trim().toLowerCase();
  const list = fonts.value;
  return query === ""
    ? list
    : list.filter((name) => name.toLowerCase().includes(query));
});

/** 选一档预设：只改观感，正文一个字不动 */
function pickPreset(code: string) {
  void setEditorFont(code);
}

/** 展开/收起「更多字体…」：第一次展开才去读系统字体（读一次就够） */
async function toggleFonts() {
  fontsOpen.value = !fontsOpen.value;
  if (!fontsOpen.value || fontsLoaded.value) return;
  fontsLoading.value = true;
  fontsError.value = "";
  try {
    fonts.value = await systemFonts();
    fontsLoaded.value = true;
  } catch (error) {
    fontsError.value = t("settings.font_failed", {
      detail: error instanceof Error ? error.message : String(error),
    });
  } finally {
    fontsLoading.value = false;
  }
}

function pickFamily(name: string) {
  void setEditorFont(`family:${name}`);
}

// ── 稿纸横线：实线 / 虚线 / 不显示 ────────────────────────────────
// 线型是作者选的一档（核心 `appearance.editor_rule_style`）；"一格多高"由 `editor/rule.ts`
// 按字号 × 行距算出来。这里只管"显示与改"——写完核心回读，编辑区立刻换。
/** 现在画哪种线（这本书生效的那份优先，没打开书看全局；读不出来就显示默认那档） */
const rule = computed(() => asRuleStyle(workValues.value ?? values.value));

/** 三档的文案键（顺序 = RULE_STYLES） */
const RULE_LABELS: Record<RuleStyle, string> = {
  solid: "settings.rule.solid",
  dashed: "settings.rule.dashed",
  none: "settings.rule.none",
};

/** 选一档：只改观感，正文一个字不动 */
function onRule(event: Event) {
  const code = (event.target as HTMLInputElement).value;
  void setRuleStyle(code);
}
</script>

<template>
  <p class="settings__group">{{ t("settings.group_typography") }}</p>
  <!-- 字体：五档常用 + 系统里装了的（可搜）。只改正文观感，不进导出。 -->
  <p class="settings__row">
    <span class="settings__label">{{ t("settings.font") }}</span>
    <span class="settings__choices">
      <label v-for="preset in FONT_PRESETS" :key="preset.code" class="settings__choice">
        <input
          type="radio"
          name="ym-font"
          :value="preset.code"
          :checked="fontValue === preset.code"
          :disabled="busy || unavailable"
          @change="pickPreset(preset.code)"
        />
        <span>{{ t(preset.labelKey) }}</span>
      </label>
      <button
        type="button"
        class="settings__button dialog__button"
        :disabled="busy || unavailable"
        @click="toggleFonts"
      >
        {{ fontsOpen ? t("settings.font_less") : t("settings.font_more") }}
      </button>
    </span>
  </p>
  <p v-if="customFamily" class="settings__hint">
    {{ t("settings.font_current", { name: customFamily }) }}
  </p>
  <div v-if="fontsOpen" class="settings__fonts">
    <input
      v-model="fontQuery"
      type="search"
      class="settings__fonts-search"
      :placeholder="t('settings.font_search')"
    />
    <p v-if="fontsLoading" class="settings__hint">{{ t("settings.font_loading") }}</p>
    <p v-else-if="fontsError" class="settings__hint">{{ fontsError }}</p>
    <p v-else-if="filteredFonts.length === 0" class="settings__hint">
      {{ t("settings.font_empty") }}
    </p>
    <ul v-else class="settings__fonts-list">
      <li v-for="name in filteredFonts" :key="name">
        <button
          type="button"
          class="settings__fonts-item"
          :class="{ 'settings__fonts-item--on': customFamily === name }"
          :disabled="busy || unavailable"
          @click="pickFamily(name)"
        >
          {{ name }}
        </button>
      </li>
    </ul>
  </div>
  <p class="settings__hint">{{ t("settings.font_hint") }}</p>

  <p v-for="knob in TYPOGRAPHY_KNOBS" :key="knob.field" class="settings__row">
    <span class="settings__label">{{ t(knob.labelKey) }}</span>
    <input
      class="settings__range"
      type="range"
      :min="knob.min"
      :max="knob.max"
      :step="knob.step"
      :value="typographyValue(knob.field)"
      :disabled="busy || unavailable"
      @input="onTypography(knob.field, $event)"
    />
    <span class="settings__value">{{ typographyLabel(knob) }}</span>
  </p>
  <!-- 示例：就地看到效果，不必切回正文（绑的是同一份变量） -->
  <p class="settings__sample" :style="sampleStyle">{{ t("settings.typography_sample") }}</p>
  <p class="settings__hint">{{ t("settings.typography_hint") }}</p>
  <button
    type="button"
    class="settings__button dialog__button"
    :disabled="busy || unavailable"
    @click="resetTypography"
  >
    {{ t("settings.typography_reset") }}
  </button>
  <!-- 稿纸横线：垫在正文底下的一层（跟着行距走）。三档与配色一样是"看一眼就知道"的选择框。 -->
  <p class="settings__group">{{ t("settings.group_paper") }}</p>
  <p class="settings__row">
    <span class="settings__label">{{ t("settings.rule_style") }}</span>
    <span class="settings__choices">
      <label v-for="style in RULE_STYLES" :key="style" class="settings__choice">
        <input
          type="radio"
          name="ym-rule"
          :value="style"
          :checked="rule === style"
          :disabled="busy || unavailable"
          @change="onRule"
        />
        <span>{{ t(RULE_LABELS[style]) }}</span>
      </label>
    </span>
  </p>
  <p class="settings__hint">{{ t("settings.rule_hint") }}</p>
</template>

<style scoped src="./settings-dialog.css"></style>

<style scoped src="./dialog.css"></style>
