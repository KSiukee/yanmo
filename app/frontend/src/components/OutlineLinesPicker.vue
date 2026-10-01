<script setup lang="ts">
// 「五线」那一格的多选卡：**这一章在推进哪几条线**（五个勾选框 + 一句说明）。
//
// 与「出场人物」那张卡（`OutlineCastPicker.vue`）分开成两件的原因：那一张是"选谁出场"，
// 这一张是"推进哪几条线"——两个变化理由（选择来源不同、能标线的段也不同）。
//
// 三条分寸：
// 1. **勾了不立刻发请求**：手上那份清单由外面管（`grid-lines.ts` 的 draft），
//    这一张卡只把"刚勾的那一条"报上去；一次编辑只发一次写（整份覆盖那条语义）；
// 2. **那句说明必须看得见**：一条都不标＝还没说，不是没推进；大纲撤回不会回滚这里
//    （用户明确要求写在界面上，不是只写在注释里）；
// 3. **回执取库里的那一份**：卡上勾的当下是手上这份，关上之后格子上摆的是核心回的那一份。
import { onBeforeUnmount } from "vue";

import { t } from "../locales/index.ts";
import { lineName, markedLines, STORY_LINES, type StoryLineCode } from "./outline-lines.ts";

const props = defineProps<{
  /** 这一段**此刻勾着**的那几条（手上那一份，还没落库） */
  marked: string[];
  /** 忙的时候不让勾：两次快速点击会各自按手上那份旧清单算，后一次把前一次盖掉 */
  busy: boolean;
}>();

const emit = defineEmits<{
  toggle: [line: string];
  close: [];
}>();

/** 这一条现在勾着没有（认不出的码在 `markedLines` 里就被丢掉了）。 */
function checked(code: StoryLineCode): boolean {
  return markedLines(props.marked).includes(code);
}

/**
 * 这张卡一卸载就等于**关上**：切页 / 收起这一屏 / 换书时，手上勾的那几下不能一声不吭地丢掉
 * （外面那句 `close` 顺手把整份落一次；已经关上过的那次是空跑）。
 */
onBeforeUnmount(() => emit("close"));
</script>

<template>
  <div class="grid__lines-pop">
    <p class="grid__lines-title">{{ t("grid.lines.title") }}</p>
    <label v-for="code in STORY_LINES" :key="code" class="grid__lines-item">
      <input
        type="checkbox"
        :checked="checked(code)"
        :disabled="busy"
        @change="emit('toggle', code)"
      />
      <span>{{ lineName(code) }}</span>
    </label>
    <!-- 用户要求写在这里的一句说明：它既说"空"的意思，也说清撤回的边界 -->
    <p class="grid__lines-note">{{ t("grid.lines.note") }}</p>
    <button type="button" class="grid__lines-done" @click="emit('close')">
      {{ t("common.close") }}
    </button>
  </div>
</template>

<style scoped src="./outline-lines.css"></style>
