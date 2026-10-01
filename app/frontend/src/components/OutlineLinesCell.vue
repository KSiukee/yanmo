<script setup lang="ts">
// 「五线」那一格：**点开勾选**（与「出场人物」那一格同一套手感：点选，不是在格子里打字）。
//
// 单独成件的原因：它是表里**第二格不是打字改的**——而且要同时摆两样东西
// （格子上的徽标 + 弹出来的那张卡）。留在 `OutlineTable.vue` 里会让那个本来就很长的
// 文件再长一截（它贴着上帝级那条线），而这一格与"表怎么摆"没有关系。
//
// 两件事由外面管（这里只画、只转发）：这一段现在标着什么（`row.lines`）、
// 卡开着没有（`open`）；点开 / 收起 / 勾一条都发事件出去，**自己不猜、不存**。
import type { OutlineRowDto } from "../api/outline.ts";
import { t } from "../locales/index.ts";
import { badgeTexts, unmarked } from "./outline-lines.ts";
import OutlineLinesPicker from "./OutlineLinesPicker.vue";

defineProps<{
  /** 这一行的行数据（只读它自己的标注：`lines`） */
  row: OutlineRowDto;
  /** 这一行的卡现在开着没有 */
  open: boolean;
  /** 忙的时候不让勾（一次编辑一次写，写到一半再点会盖） */
  busy: boolean;
  /** 卡上此刻勾着的那几条（手上那一份，还没落库） */
  draft: string[];
  /** 这一行能不能标（卷与场景卡不能——见 `cellEditable`）：不能就只画，不摆编辑入口 */
  editable: boolean;
}>();

const emit = defineEmits<{
  open: [];
  close: [];
  toggle: [line: string];
}>();
</script>

<template>
  <div class="grid__lines">
    <!-- 不能标的行（卷 / 场景卡）：只摆画出来的徽标，**不摆编辑入口** -->
    <span v-if="!editable" class="grid__lines-open grid__lines-open--empty">
      <span v-if="unmarked(row.lines)" class="grid__lines-none">{{ t("grid.lines.none") }}</span>
      <span v-for="text in badgeTexts(row.lines)" :key="text" class="grid__lines-badge">
        {{ text }}
      </span>
    </span>
    <button
      v-else
      type="button"
      class="grid__lines-open"
      :class="{ 'grid__lines-open--empty': unmarked(row.lines) }"
      :title="t('grid.lines.title')"
      @click="emit('open')"
    >
      <!-- 一条都没标：淡淡的「未标」——不是空白，也不是"0 条" -->
      <span v-if="unmarked(row.lines)" class="grid__lines-none">{{ t("grid.lines.none") }}</span>
      <span v-for="text in badgeTexts(row.lines)" :key="text" class="grid__lines-badge">
        {{ text }}
      </span>
    </button>
    <OutlineLinesPicker
      v-if="open && editable"
      :marked="draft"
      :busy="busy"
      @toggle="(line) => emit('toggle', line)"
      @close="emit('close')"
    />
  </div>
</template>

<style scoped src="./outline-lines.css"></style>
