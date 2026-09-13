<script setup lang="ts">
// 码字日历：**今天写了多少 / 这个月哪天写了多少 / 每天想写多少**。
//
// 视图只负责"排格子与显示"：账本、连续天数在核心（`store::writing`），
// 格子怎么排、进度怎么折在 `editor/writing-days.ts`（纯函数，有单测），
// 目标那份偏好在 `editor/writing-goal.ts`。
import { computed, ref, watch } from "vue";

import { t } from "../locales/index.ts";
import type { EditorSession } from "../editor/session";
import {
  countByDay,
  intensityOf,
  localDayKey,
  monthGrid,
  monthRange,
  peakOf,
  pickDayCount,
  sumBetween,
  weekRange,
} from "../editor/writing-days";

const props = defineProps<{ session: EditorSession }>();
const writing = props.session.writing;
const { goal, defaultGoal, scope, month, days, streak, busy, close, setScope, shiftMonth, setGoal } =
  writing;
const caliber = props.session.caliber;
const workId = props.session.workId;

/** 今天（本地）——格子上的"今天"那一圈靠它 */
const todayKey = localDayKey(new Date());
/** 日期 → 当前口径的字数（核心给的是"有记录的日子"） */
const counts = computed(() => countByDay(days.value, caliber.value));
const peak = computed(() => peakOf(counts.value));
const weeks = computed(() => monthGrid(month.value.year, month.value.month, counts.value, todayKey));
const weekdays = [
  "writing.weekday_mon",
  "writing.weekday_tue",
  "writing.weekday_wed",
  "writing.weekday_thu",
  "writing.weekday_fri",
  "writing.weekday_sat",
  "writing.weekday_sun",
];

const weekTotal = computed(() => {
  const range = weekRange(new Date());
  return sumBetween(counts.value, range.from, range.to);
});
const monthTotal = computed(() => {
  const range = monthRange(month.value.year, month.value.month);
  return sumBetween(counts.value, range.from, range.to);
});
const monthTitle = computed(() =>
  t("writing.month_title", { year: month.value.year, month: month.value.month }),
);

/** 今天这一格的字数：**跟着面板当前看谁走**（"全部作品"就是合计） */
const todayCount = computed(() =>
  pickDayCount(days.value.find((day) => day.day === todayKey) ?? null, caliber.value),
);
const cellTitle = (day: string | null, count: number) =>
  day === null ? "" : t("writing.cell_title", { day, count });

/** 目标输入框：进面板时按当前生效的那一份填上（清空 = 不设目标） */
const goalDraft = ref("");
const goalTarget = ref<"work" | "default">("work");
watch(
  () => goal.value,
  (value) => {
    goalDraft.value = value === null ? "" : String(value);
  },
  { immediate: true },
);
// 没打开书时只能设默认目标（"只设这本书"没有对象）
watch(workId, (id) => { goalTarget.value = id === null ? "default" : "work"; }, { immediate: true });

const hasOverride = computed(
  () => workId.value !== null && goal.value !== null && goal.value !== defaultGoal.value,
);

function saveGoal(): void {
  const parsed = Number.parseInt(goalDraft.value.trim(), 10);
  void setGoal(Number.isFinite(parsed) && parsed > 0 ? parsed : 0, goalTarget.value);
}

/** 跟默认：把这本书的单独设置清掉（核心那边写 0 = 回到继承全局） */
function followDefault(): void {
  void setGoal(0, "work");
}
</script>

<template>
  <div class="writing dialog" @click.self="close">
    <section class="writing__box dialog__box">
      <header class="writing__head dialog__head">
        <h2 class="writing__title dialog__title">{{ t("writing.title") }}</h2>
        <button
          type="button" class="writing__button dialog__button"
          :title="t('writing.close_title')" @click="close"
        >{{ t("common.close") }}</button>
      </header>

      <!-- 今日 + 连续天数：一眼看到"今天写了没有、连着写了多久" -->
      <p class="writing__headline">
        <span class="writing__today">{{ t("editor.today", { count: todayCount }) }}</span>
        <span class="writing__streak">{{ t("writing.streak", { days: streak }) }}</span>
      </p>

      <div class="writing__bar">
        <span class="writing__scope-label">{{ t("writing.scope") }}</span>
        <button
          type="button" class="writing__button dialog__button"
          :class="{ 'writing__button--on': scope === 'work' }" :disabled="workId === null"
          @click="void setScope('work')"
        >{{ t("writing.scope_work") }}</button>
        <button
          type="button" class="writing__button dialog__button"
          :class="{ 'writing__button--on': scope === 'all' }" @click="void setScope('all')"
        >{{ t("writing.scope_all") }}</button>
        <span class="writing__spacer" />
        <button
          type="button" class="writing__button dialog__button" :title="t('writing.prev_month')"
          :disabled="busy" @click="void shiftMonth(-1)"
        >‹</button>
        <span class="writing__month">{{ monthTitle }}</span>
        <button
          type="button" class="writing__button dialog__button" :title="t('writing.next_month')"
          :disabled="busy" @click="void shiftMonth(1)"
        >›</button>
      </div>

      <div class="writing__grid">
        <span v-for="key in weekdays" :key="key" class="writing__weekday">{{ t(key) }}</span>
        <template v-for="(week, index) in weeks" :key="index">
          <span
            v-for="(cell, day) in week" :key="`${index}-${day}`" class="writing__cell"
            :class="[
              cell.day === null ? 'writing__cell--pad' : `writing__cell--l${intensityOf(cell.count, peak)}`,
              { 'writing__cell--today': cell.today },
            ]"
            :title="cellTitle(cell.day, cell.count)"
          >{{ cell.day === null ? "" : Number(cell.day.slice(8, 10)) }}</span>
        </template>
      </div>

      <p class="writing__totals">
        <span>{{ t("writing.week", { count: weekTotal }) }}</span>
        <span>{{ t("writing.month", { count: monthTotal }) }}</span>
        <span class="writing__legend">
          {{ t("writing.legend_less") }}
          <span class="writing__cell writing__cell--l1" />
          <span class="writing__cell writing__cell--l2" />
          <span class="writing__cell writing__cell--l3" />
          <span class="writing__cell writing__cell--l4" />
          {{ t("writing.legend_more") }}
        </span>
      </p>
      <p v-if="days.length === 0" class="writing__hint">{{ t("writing.empty_month") }}</p>

      <p class="writing__group">{{ t("writing.goal") }}</p>
      <p class="writing__effective">
        {{ goal === null ? t("writing.goal_none") : t("writing.goal_effective", { count: goal }) }}
      </p>
      <form class="writing__goal" @submit.prevent="saveGoal">
        <input
          v-model="goalDraft" class="writing__input dialog__input" type="number" min="0"
          :placeholder="t('writing.goal_placeholder')"
        />
        <select v-model="goalTarget" class="writing__button dialog__button">
          <option value="work" :disabled="workId === null">{{ t("writing.goal_for_work") }}</option>
          <option value="default">{{ t("writing.goal_for_default") }}</option>
        </select>
        <button type="submit" class="writing__button dialog__button" :disabled="busy">
          {{ t("common.save") }}
        </button>
        <button
          v-if="hasOverride" type="button" class="writing__button dialog__button"
          :title="t('writing.goal_follow_title')" :disabled="busy" @click="followDefault"
        >{{ t("writing.goal_follow") }}</button>
      </form>
      <p class="writing__hint">
        {{ goalTarget === "work" ? t("writing.goal_note_work") : t("writing.goal_note_default") }} ·
        {{ t("writing.goal_hint") }}
      </p>
    </section>
  </div>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./writing-dialog.css"></style>
