<script setup lang="ts">
// 排版清理面板：**只列建议，一条都不改**——作者勾了哪几处，才改哪几处。
//
// 分工：规则清单、命中、应用与"动手前先留底"全在 editor/typeset.ts（可脱离界面单测）；
// 这个文件只管"长什么样"（与设置面板分开：那边是偏好，这边是一次动作）。
import { computed } from "vue";

import { has, t } from "../locales/index.ts";
import { groupChanges } from "../editor/typeset";
import type { TypesetNotice } from "../api/core";
import type { EditorSession } from "../editor/session";

const props = defineProps<{ session: EditorSession }>();
const {
  rules,
  changes,
  notices,
  picked,
  quoteStyle,
  note,
  busy,
  apply,
  close,
  toggleChange,
  toggleRule,
  setQuoteStyle,
} = props.session.typeset;

/** 规则名与风险档的句子都在字典里（核心只给码）。 */
function ruleName(code: string): string {
  const key = `typeset.rule.${code}`;
  return t(has(key) ? key : code);
}

function tierText(tier: string): string {
  const key = `typeset.tier_${tier}`;
  return t(has(key) ? key : tier);
}

/** 符号对与"缺哪边"的名字同样在字典里（核心只给 quote_double / unclosed 这样的码）。 */
function markText(mark: string): string {
  const key = `typeset.mark.${mark}`;
  return t(has(key) ? key : mark);
}

function sideText(side: string): string {
  const key = `typeset.side.${side}`;
  return t(has(key) ? key : side);
}

function noticeText(item: TypesetNotice): string {
  return t(`typeset.notice.${item.rule}`, {
    paragraph: item.paragraph,
    mark: markText(item.mark),
    side: sideText(item.side),
  });
}

/** 空串（插入 / 删除）不能直接显示——两个空格并排，看不出发生了什么。 */
function shown(text: string): string {
  if (text === "") return t("typeset.nothing");
  if (text.trim() === "") return t("typeset.space_mark");
  return text;
}

function toggleWholeRule(code: string, event: Event) {
  const box = event.target as HTMLInputElement;
  toggleRule(code, box.checked);
}

const groups = computed(() => groupChanges(rules.value, changes.value, picked.value));
const pickedCount = computed(() => picked.value.size);
</script>

<template>
  <div class="typeset dialog" @click.self="close()">
    <section class="typeset__box dialog__box">
      <header class="dialog__head">
        <h2 class="dialog__title">{{ t("typeset.title") }}</h2>
        <button type="button" class="dialog__button" @click="close()">
          {{ t("typeset.cancel") }}
        </button>
      </header>

      <p class="dialog__note">{{ t("typeset.lead") }}</p>
      <p class="typeset__scope">{{ t("typeset.scope") }}</p>

      <!-- 只报告、不给改法的那一类（缺一半的引号）：列出来，但绝不替作者猜着补 -->
      <section v-if="notices.length > 0" class="typeset__notices">
        <h3 class="typeset__notices-title">
          {{ t("typeset.notices_title") }}
          <span class="typeset__count">{{ t("typeset.count", { count: notices.length }) }}</span>
        </h3>
        <ul class="typeset__notice-list">
          <li v-for="(item, index) in notices" :key="index" class="typeset__notice">
            <span>{{ noticeText(item) }}</span>
            <span class="typeset__faint">
              {{ item.context_before }}<span class="typeset__hole">{{ t("typeset.hole") }}</span>{{ item.context_after }}
            </span>
          </li>
        </ul>
      </section>

      <p v-if="groups.length === 0" class="dialog__empty">
        {{ notices.length > 0 ? t("typeset.empty_with_notices") : t("typeset.empty") }}
      </p>

      <div v-else class="typeset__body">
        <ul class="typeset__groups">
          <li v-for="group in groups" :key="group.rule.code" class="typeset__group">
            <label class="typeset__head">
              <input
                type="checkbox"
                :checked="group.picked === group.items.length"
                @change="toggleWholeRule(group.rule.code, $event)"
              />
              <span class="typeset__name">{{ ruleName(group.rule.code) }}</span>
              <span class="typeset__count">{{ t("typeset.count", { count: group.items.length }) }}</span>
            </label>
            <p class="typeset__tier">{{ tierText(group.rule.tier) }}</p>
          </li>
        </ul>

        <ul class="typeset__list">
          <li v-for="group in groups" :key="group.rule.code" class="typeset__items">
            <label v-for="item in group.items" :key="item.index" class="typeset__item">
              <input type="checkbox" :checked="item.picked" @change="toggleChange(item.index)" />
              <span class="typeset__where">
                {{ t("typeset.paragraph", { index: item.change.paragraph }) }}
              </span>
              <span class="typeset__line">
                <span class="typeset__faint">{{ item.change.context_before }}</span>
                <span class="typeset__old">{{ shown(item.change.before) }}</span>
                <span class="typeset__arrow">→</span>
                <span class="typeset__new">{{ shown(item.change.after) }}</span>
                <span class="typeset__faint">{{ item.change.context_after }}</span>
              </span>
            </label>
          </li>
        </ul>
      </div>

      <footer class="typeset__foot">
        <span class="typeset__quote">
          {{ t("typeset.quote_style") }}
          <label>
            <input
              type="radio"
              name="typeset-quote"
              :checked="quoteStyle === 'curly'"
              @change="setQuoteStyle('curly')"
            />
            {{ t("typeset.quote.curly") }}
          </label>
          <label>
            <input
              type="radio"
              name="typeset-quote"
              :checked="quoteStyle === 'corner'"
              @change="setQuoteStyle('corner')"
            />
            {{ t("typeset.quote.corner") }}
          </label>
        </span>
        <span class="typeset__note">{{ note }}</span>
        <span class="typeset__selected">{{ t("typeset.selected", { count: pickedCount }) }}</span>
        <button
          type="button"
          class="dialog__button typeset__apply"
          :disabled="busy"
          @click="void apply()"
        >
          {{ t("typeset.apply", { count: pickedCount }) }}
        </button>
      </footer>
    </section>
  </div>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./typeset-dialog.css"></style>
