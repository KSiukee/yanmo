<script setup lang="ts">
// 正文背景那一栏：**底色 / 稿纸底纹 / 自定义图片（不透明度可调）**。
//
// 为什么单独成一个组件：设置面板本身贴着 400 行的棘轮，而这一栏自成一体——
// 四档底色 + 四档底纹 + 一张图与它的两个旋钮，与别的设置项没有耦合。
//
// 三条分寸（与核心那一边对着）：
// - **档位与换算一律向 `editor/background.ts` 要**（组件里不写死颜色与格距）；
// - **图片由壳搬进数据目录**：这儿只递标题与筛选标签，回来的只有文件名与一段 data URL
//   ——组件里没有任何"路径"概念，也没有文件系统能力；
// - **提示只提示**：图太实、或"有色底 + 底纹"叠一起时，这里只说一句代价，不替作者改。
import { computed } from "vue";

import { t } from "../locales/index.ts";
import type { EditorSession } from "../editor/session";
import {
  asBgColor,
  asBgFit,
  asBgLayout,
  asBgOpacity,
  bgImageName,
  BG_COLORS,
  BG_FITS,
  BG_LAYOUTS,
  readabilityHint,
  type BgColor,
  type BgFit,
  type BgLayout,
} from "../editor/background.ts";

const props = defineProps<{ session: EditorSession }>();
const { values, workValues, busy, background } = props.session.appearance;
const {
  setBackgroundColor,
  setBackgroundLayout,
  setBackgroundFit,
  setBackgroundOpacity,
  pickBackground,
  clearBackground,
} = props.session.appearance;

/** 现在生效的那一份：这本书的覆盖优先，没打开书看全局（与编辑区绑的是同一份） */
const current = computed(() => workValues.value ?? values.value);
const unavailable = computed(() => values.value === null);

/** 四档底色的文案键（顺序 = BG_COLORS） */
const COLOR_LABELS: Record<BgColor, string> = {
  paper: "settings.bg.paper",
  cream: "settings.bg.cream",
  green: "settings.bg.green",
  gray: "settings.bg.gray",
};

/** 四档底纹的文案键（顺序 = BG_LAYOUTS） */
const LAYOUT_LABELS: Record<BgLayout, string> = {
  none: "settings.bg.layout_none",
  grid: "settings.bg.layout_grid",
  dots: "settings.bg.layout_dots",
  grain: "settings.bg.layout_grain",
};

/** 三档铺法的文案键（顺序 = BG_FITS） */
const FIT_LABELS: Record<BgFit, string> = {
  cover: "settings.bg.fit_cover",
  contain: "settings.bg.fit_contain",
  tile: "settings.bg.fit_tile",
};

const color = computed(() => asBgColor(current.value));
const layout = computed(() => asBgLayout(current.value));
const fit = computed(() => asBgFit(current.value));
const opacity = computed(() => asBgOpacity(current.value));

/** 现在有没有图：**以壳那边的回执为准**（库里有名字但文件没了时，这里会说"图没了"） */
const imageName = computed(() => background.value?.name ?? bgImageName(current.value));
const imageShown = computed(() => (background.value?.data_url ? background.value.data_url : null));
const imageGone = computed(
  () => imageName.value !== null && background.value !== null && !background.value.present,
);

/** 可读性提示（只提示，不挡人） */
const hint = computed(() => readabilityHint(current.value));

/** 这张图多大（MB，一位小数）——壳在选图那一次就告诉了我们 */
const imageSize = computed(() => {
  const bytes = background.value?.bytes;
  if (typeof bytes !== "number" || bytes <= 0) return "";
  return t("settings.bg.size", { size: Math.round((bytes / (1024 * 1024)) * 10) / 10 });
});

/** 选档：只改观感，正文一个字不动（写完核心回读，编辑区立刻换） */
function onColor(event: Event) {
  void setBackgroundColor((event.target as HTMLInputElement).value);
}
function onLayout(event: Event) {
  void setBackgroundLayout((event.target as HTMLInputElement).value);
}
function onFit(event: Event) {
  void setBackgroundFit((event.target as HTMLInputElement).value);
}
/** 不透明度：0 是正经取值（图留着但看不见），所以不能拿 0 当"清掉" */
function onOpacity(event: Event) {
  const value = Number.parseInt((event.target as HTMLInputElement).value, 10);
  if (Number.isFinite(value)) void setBackgroundOpacity(value);
}
</script>

<template>
  <p class="settings__group">{{ t("settings.group_background") }}</p>

  <!-- 底色：与配色那一栏一样是"看一眼就知道"的选择框 -->
  <p class="settings__row">
    <span class="settings__label">{{ t("settings.bg.color") }}</span>
    <span class="settings__choices">
      <label v-for="code in BG_COLORS" :key="code" class="settings__choice">
        <input
          type="radio"
          name="ym-bg-color"
          :value="code"
          :checked="color === code"
          :disabled="busy || unavailable"
          @change="onColor"
        />
        <span>{{ t(COLOR_LABELS[code]) }}</span>
      </label>
    </span>
  </p>

  <!-- 底纹：垫在字底下的一层（一格多高由稿纸横线那一把尺子算：字号 × 行距） -->
  <p class="settings__row">
    <span class="settings__label">{{ t("settings.bg.layout") }}</span>
    <span class="settings__choices">
      <label v-for="code in BG_LAYOUTS" :key="code" class="settings__choice">
        <input
          type="radio"
          name="ym-bg-layout"
          :value="code"
          :checked="layout === code"
          :disabled="busy || unavailable"
          @change="onLayout"
        />
        <span>{{ t(LAYOUT_LABELS[code]) }}</span>
      </label>
    </span>
  </p>
  <p class="settings__hint">{{ t("settings.bg.hint") }}</p>

  <!-- 自定义图片：壳把图复制进数据目录（跟着稿子走），界面只拿到文件名与一段渲染用的画面 -->
  <p class="settings__row">
    <span class="settings__label">{{ t("settings.bg.image") }}</span>
    <button
      type="button"
      class="settings__button dialog__button"
      :disabled="busy || unavailable"
      @click="pickBackground"
    >
      {{ imageName ? t("settings.bg.replace") : t("settings.bg.pick") }}
    </button>
    <button
      v-if="imageName"
      type="button"
      class="settings__button dialog__button"
      :disabled="busy || unavailable"
      @click="clearBackground"
    >
      {{ t("settings.bg.clear") }}
    </button>
    <span v-if="imageName" class="settings__value">{{ imageName }} {{ imageSize }}</span>
  </p>

  <!-- 图没了：壳在打开面板时问过一次，文件被删掉 / 搬走时如实说，并说明回退 -->
  <p v-if="imageGone" class="settings__hint settings__hint--warn">
    {{ t("settings.bg.image_gone") }}
  </p>

  <!-- 有图才给这两个旋钮：没图时摆着只会让人以为"调了没反应" -->
  <template v-if="imageName">
    <p class="settings__row">
      <span class="settings__label">{{ t("settings.bg.opacity") }}</span>
      <input
        class="settings__range"
        type="range"
        min="0"
        max="100"
        step="5"
        :value="opacity"
        :disabled="busy || unavailable"
        @input="onOpacity"
      />
      <span class="settings__value">{{ opacity }}%</span>
    </p>
    <p class="settings__row">
      <span class="settings__label">{{ t("settings.bg.fit") }}</span>
      <span class="settings__choices">
        <label v-for="code in BG_FITS" :key="code" class="settings__choice">
          <input
            type="radio"
            name="ym-bg-fit"
            :value="code"
            :checked="fit === code"
            :disabled="busy || unavailable"
            @change="onFit"
          />
          <span>{{ t(FIT_LABELS[code]) }}</span>
        </label>
      </span>
    </p>
    <p v-if="hint === 'opacity'" class="settings__hint settings__hint--warn">
      {{ t("settings.bg.hint_opacity") }}
    </p>
    <p v-else-if="hint === 'busy'" class="settings__hint settings__hint--warn">
      {{ t("settings.bg.hint_busy") }}
    </p>
    <p class="settings__hint">{{ t("settings.bg.image_hint") }}</p>
  </template>
  <!-- 预览：就地看到这一层长得什么样（画的是同一段 data URL 与同一份不透明度） -->
  <p
    v-if="imageShown"
    class="settings__preview"
    :style="{ backgroundImage: `url(${imageShown})`, opacity: String(opacity / 100) }"
    :aria-label="t('settings.bg.preview')"
  ></p>
</template>

<style scoped src="./settings-dialog.css"></style>
