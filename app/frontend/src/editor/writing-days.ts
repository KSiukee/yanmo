// 码字日历与今日进度的**显示逻辑**：算哪几天、格子怎么排、进度到哪。
//
// 分寸（别在这里重复核心的事）：
// - **哪一天写在哪一格、连续了几天**是核心的事（账本在 `writing_days`，见 `store::writing`），
//   这里只把核心给的日子排成月视图、把一个数折成进度；
// - **取哪个口径的数**跟状态栏同一套（口径码由核心落定，界面不自己排对照表）。
//
// 纯函数、不碰 DOM、不 import 界面：可以脱离浏览器单测。

import type { WritingDay } from "../api/core";

/** 某一天按当前口径取数——与状态栏那个数字同一套读法。 */
export function pickDayCount(
  day: WritingDay | null | undefined,
  caliber: string | null | undefined,
): number {
  if (!day) return 0;
  switch (caliber) {
    case "chars_no_punct":
      return day.chars_no_punct;
    case "words":
      return day.words;
    default:
      return day.chars;
  }
}

/**
 * 今日进度：`今日 / 目标`，夹在 0–1。
 *
 * - 没设目标 → `null`（界面就不画进度，只显示今日写了多少）；
 * - 目标非正数当没设（与核心同一条规矩）；
 * - **超额封顶 1**：写了 5000 / 2000 也不把进度条撑出格子。
 */
export function goalProgress(today: number, goal: number | null | undefined): number | null {
  if (goal === null || goal === undefined || !Number.isFinite(goal) || goal <= 0) return null;
  return Math.min(1, Math.max(0, today / goal));
}

/** 进度条够不够"今天达标了"（界面用它换个颜色/一句话）。 */
export function goalReached(today: number, goal: number | null | undefined): boolean {
  const ratio = goalProgress(today, goal);
  return ratio !== null && ratio >= 1;
}

/** 本地日期键 `YYYY-MM-DD`（**不看时间**，只按本地日历）。 */
export function localDayKey(date: Date): string {
  const y = date.getFullYear();
  const m = `${date.getMonth() + 1}`.padStart(2, "0");
  const d = `${date.getDate()}`.padStart(2, "0");
  return `${y}-${m}-${d}`;
}

/** 某年某月有几天（`month` 是 1–12）。 */
export function daysInMonth(year: number, month: number): number {
  return new Date(year, month, 0).getDate();
}

/** 加减月份（跨年也对）。 */
export function shiftMonth(
  year: number,
  month: number,
  delta: number,
): { year: number; month: number } {
  const zero = year * 12 + (month - 1) + delta;
  return { year: Math.floor(zero / 12), month: (zero % 12 + 12) % 12 + 1 };
}

/** 某月的首尾日期键（用于向核心取这一段）。 */
export function monthRange(year: number, month: number): { from: string; to: string } {
  const pad = (n: number) => `${n}`.padStart(2, "0");
  return {
    from: `${year}-${pad(month)}-01`,
    to: `${year}-${pad(month)}-${pad(daysInMonth(year, month))}`,
  };
}

/** 本周（周一起算）的首尾日期键。 */
export function weekRange(today: Date): { from: string; to: string } {
  // getDay()：周日 = 0。往前推到周一：周日要退 6 天，其余退 (day-1) 天
  const back = (today.getDay() + 6) % 7;
  const start = new Date(today.getFullYear(), today.getMonth(), today.getDate() - back);
  const end = new Date(start.getFullYear(), start.getMonth(), start.getDate() + 6);
  return { from: localDayKey(start), to: localDayKey(end) };
}

/** 把核心给的"有记录的日子"折成 `日期 → 当前口径的字数`。 */
export function countByDay(
  days: WritingDay[],
  caliber: string | null | undefined,
): Map<string, number> {
  const out = new Map<string, number>();
  for (const day of days) {
    out.set(day.day, (out.get(day.day) ?? 0) + pickDayCount(day, caliber));
  }
  return out;
}

/** 闭区间内的合计（本周 / 本月那两行小计用它）。 */
export function sumBetween(counts: Map<string, number>, from: string, to: string): number {
  let total = 0;
  for (const [day, count] of counts) {
    if (day >= from && day <= to) total += count;
  }
  return total;
}

/** 月视图的一格。 */
export interface CalendarCell {
  /** `null` = 补白格（那一周里不属于这个月的日子） */
  day: string | null;
  /** 当前口径下的字数（补白格为 0） */
  count: number;
  /** 是不是今天（今天要描一圈，哪怕一个字没写） */
  today: boolean;
}

/** 月视图：**周一起算**，一周一行，前后补空格子。 */
export function monthGrid(
  year: number,
  month: number,
  counts: Map<string, number>,
  today: string,
): CalendarCell[][] {
  const pad = (n: number) => `${n}`.padStart(2, "0");
  // 这个月 1 号是周几（0 = 周日）→ 前面要补几格（周一起算）
  const lead = (new Date(year, month - 1, 1).getDay() + 6) % 7;
  const total = daysInMonth(year, month);
  const cells: CalendarCell[] = [];
  for (let i = 0; i < lead; i += 1) cells.push({ day: null, count: 0, today: false });
  for (let d = 1; d <= total; d += 1) {
    const day = `${year}-${pad(month)}-${pad(d)}`;
    cells.push({ day, count: counts.get(day) ?? 0, today: day === today });
  }
  while (cells.length % 7 !== 0) cells.push({ day: null, count: 0, today: false });
  const weeks: CalendarCell[][] = [];
  for (let i = 0; i < cells.length; i += 7) weeks.push(cells.slice(i, i + 7));
  return weeks;
}

/** 格子颜色的档位（0 = 这天没写；1–4 由深到浅往上走，阈值交给界面配色用）。 */
export function intensityOf(count: number, peak: number): 0 | 1 | 2 | 3 | 4 {
  if (count <= 0 || peak <= 0) return 0;
  const ratio = count / peak;
  if (ratio >= 0.75) return 4;
  if (ratio >= 0.5) return 3;
  if (ratio >= 0.25) return 2;
  return 1;
}

/** 区间里最大的那一天（画热力深浅的基准；全空就是 0）。 */
export function peakOf(counts: Map<string, number>): number {
  let peak = 0;
  for (const count of counts.values()) {
    if (count > peak) peak = count;
  }
  return peak;
}

/** 今天往后数 `days` 天的日期键（取日历区间用；`days` 传负数就是往前）。 */
export function dayKeyOffset(today: Date, days: number): string {
  const shifted = new Date(today.getFullYear(), today.getMonth(), today.getDate() + days);
  return localDayKey(shifted);
}
