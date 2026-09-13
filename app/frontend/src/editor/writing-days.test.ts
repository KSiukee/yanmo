// 码字日历的显示逻辑：取数、进度、月视图排版。
//
// 这里**不测"哪一天写了多少"**——账本与连续天数在核心（`store::writing` 有单测）。
// 这里只测界面这一层：口径取数对不对、进度怎么折、格子怎么排、跨月跨年算不算对。

import { test } from "node:test";
import assert from "node:assert/strict";

import type { WritingDay } from "../api/core";
import {
  countByDay,
  dayKeyOffset,
  daysInMonth,
  goalProgress,
  goalReached,
  intensityOf,
  localDayKey,
  monthGrid,
  monthRange,
  peakOf,
  pickDayCount,
  shiftMonth,
  sumBetween,
  weekRange,
} from "./writing-days.ts";

const DAY: WritingDay = { day: "2026-09-16", chars: 1200, chars_no_punct: 900, words: 700 };

test("按当前口径取数，认不出来的口径退回逐字", () => {
  assert.equal(pickDayCount(DAY, "chars"), 1200);
  assert.equal(pickDayCount(DAY, "chars_no_punct"), 900);
  assert.equal(pickDayCount(DAY, "words"), 700);
  assert.equal(pickDayCount(DAY, "不认识的码"), 1200);
  assert.equal(pickDayCount(null, "chars"), 0, "还没问回来就是 0，界面不显示空");
});

test("进度：没设目标不画，超额封顶，负数目标当没设", () => {
  assert.equal(goalProgress(500, null), null);
  assert.equal(goalProgress(500, 0), null);
  assert.equal(goalProgress(500, -1), null);
  assert.equal(goalProgress(500, 1000), 0.5);
  assert.equal(goalProgress(5000, 1000), 1, "写超了也不把进度条撑出格子");
  assert.equal(goalProgress(0, 1000), 0);
  assert.equal(goalReached(1000, 1000), true);
  assert.equal(goalReached(999, 1000), false);
  assert.equal(goalReached(999, null), false, "没目标就谈不上达标");
});

test("日期键只按本地日历算，不跟着时分秒跑", () => {
  assert.equal(localDayKey(new Date(2026, 8, 16, 23, 59, 59)), "2026-09-16");
  assert.equal(localDayKey(new Date(2026, 0, 1, 0, 0, 0)), "2026-01-01");
  assert.equal(dayKeyOffset(new Date(2026, 8, 16), -7), "2026-09-09", "往前一周");
  assert.equal(dayKeyOffset(new Date(2026, 8, 30), 3), "2026-10-03", "跨月也对");
});

test("月份算术：闰年、跨年、首尾日期", () => {
  assert.equal(daysInMonth(2024, 2), 29, "闰年二月 29 天");
  assert.equal(daysInMonth(2025, 2), 28);
  assert.equal(daysInMonth(2026, 9), 30);
  assert.deepEqual(shiftMonth(2026, 1, -1), { year: 2025, month: 12 }, "往前跨年");
  assert.deepEqual(shiftMonth(2026, 12, 1), { year: 2027, month: 1 }, "往后跨年");
  assert.deepEqual(shiftMonth(2026, 9, 12), { year: 2027, month: 9 }, "一次跳一年也对");
  assert.deepEqual(monthRange(2026, 9), { from: "2026-09-01", to: "2026-09-30" });
  assert.deepEqual(monthRange(2024, 2), { from: "2024-02-01", to: "2024-02-29" });
});

test("本周从周一起算（周日属于上一周的最后一天）", () => {
  // 2026-09-16 是周三 → 本周 09-14（周一）到 09-20（周日）
  assert.deepEqual(weekRange(new Date(2026, 8, 16)), { from: "2026-09-14", to: "2026-09-20" });
  // 2026-09-13 是周日 → 归属 09-07 那一周
  assert.deepEqual(weekRange(new Date(2026, 8, 13)), { from: "2026-09-07", to: "2026-09-13" });
  // 2026-09-14 周一本身
  assert.deepEqual(weekRange(new Date(2026, 8, 14)), { from: "2026-09-14", to: "2026-09-20" });
});

test("折成日期→字数：同一天多条记录（不该有）也会累加", () => {
  const counts = countByDay(
    [DAY, { ...DAY, chars: 300, chars_no_punct: 300, words: 300 }],
    "chars",
  );
  assert.equal(counts.get("2026-09-16"), 1500);
  assert.equal(sumBetween(counts, "2026-09-15", "2026-09-17"), 1500, "闭区间");
  assert.equal(sumBetween(counts, "2026-09-17", "2026-09-20"), 0, "区间外不算");
  assert.equal(peakOf(counts), 1500);
});

test("月视图：周一起算、补空格、每周正好七格", () => {
  const counts = new Map([["2026-09-16", 500]]);
  const weeks = monthGrid(2026, 9, counts, "2026-09-16");
  // 2026-09-01 是周二 → 前面补 1 格
  assert.equal(weeks[0][0].day, null, "周一那格是补白（1 号是周二）");
  assert.equal(weeks[0][1].day, "2026-09-01");
  assert.equal(weeks[0][1].count, 0);
  for (const week of weeks) assert.equal(week.length, 7, "每周正好七格");
  const flat = weeks.flat().filter((cell) => cell.day !== null);
  assert.equal(flat.length, 30, "九月 30 天，一天不多一天不少");
  assert.equal(flat[15].day, "2026-09-16");
  assert.equal(flat[15].count, 500);
  assert.equal(flat[15].today, true);
  assert.equal(flat[14].today, false);
});

test("月视图：闰年二月与补白总数都对得上", () => {
  const weeks = monthGrid(2024, 2, new Map(), "2024-02-29");
  const flat = weeks.flat().filter((cell) => cell.day !== null);
  assert.equal(flat.length, 29);
  assert.equal(flat[28].today, true, "闰日那天是今天");
  // 2024-02-01 是周四 → 前面补 3 格
  assert.equal(weeks[0][3].day, "2024-02-01");
  for (const week of weeks) assert.equal(week.length, 7);
});

test("热力深浅：没有记录就是最浅，四档按当月峰值分", () => {
  assert.equal(intensityOf(0, 1000), 0);
  assert.equal(intensityOf(10, 0), 0, "整月没写就全浅");
  assert.equal(intensityOf(1000, 1000), 4);
  assert.equal(intensityOf(600, 1000), 3);
  assert.equal(intensityOf(300, 1000), 2);
  assert.equal(intensityOf(100, 1000), 1);
});
