// 给人看的格式：字数怎么念、时间怎么念。
//
// 单独一小块的理由：这些是**纯展示口径**，改了它不该牵动目录树或书架的逻辑；
// 反过来，谁也不该为了显示一个数字去 import 别人的状态机。

import { t } from "../locales/index.ts";
import { countUnitKey, pickCount, type Counts } from "./wordcount.ts";

/** 字数给人看：一万以下报原数，一万以上报「x.x万」（一位小数）。 */
export function formatWords(words: number): string {
  if (!Number.isFinite(words)) return t("display.words_unknown");
  if (words < 10000) return String(words);
  return t("display.words_wan", { value: Math.round(words / 1000) / 10 });
}

/**
 * 容量给人看：KB / MB / GB，一位小数。
 *
 * **只在展示层换算一次**——调用方别自己除（上一版备份面板就是这么错的：除错量纲，
 * 把几十 GB 的剩余空间显示成「11 MB」，差点把作者吓一跳）。
 */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "";
  const gb = bytes / 1024 ** 3;
  if (gb >= 1) return `${Math.round(gb * 10) / 10} GB`;
  const mb = bytes / 1024 ** 2;
  if (mb >= 1) return `${Math.round(mb)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

/**
 * 两个时间戳相差几个**本地日历日**（正数 = `later` 在后）。
 *
 * 不能拿毫秒差直接除 86400000：那算的是"过去 24 小时"，而作者眼里的"今天/昨天"是**本地
 * 日历日**——凌晨 0:30 看昨晚 23:50 打开的书，毫秒差只有 40 分钟，却该说"昨天"。
 * （2026-09-15 代码质量评审：轻微 27）
 *
 * 先把两边的本地年月日取出来，再按 UTC 的同一天算差：夏令时那两天（23/25 小时）也不会
 * 算歪，`Math.round` 兜住那一个小时的零头。
 */
export function localDaysBetween(earlier: number, later: number): number {
  const a = new Date(earlier);
  const b = new Date(later);
  const dayA = Date.UTC(a.getFullYear(), a.getMonth(), a.getDate());
  const dayB = Date.UTC(b.getFullYear(), b.getMonth(), b.getDate());
  return Math.round((dayB - dayA) / 86_400_000);
}

/** 最近打开时间给人看：今天 / 昨天 / N 天前 / 具体日期；从没打开过就直说。 */
export function formatWhen(ms: number | null, now: number = Date.now()): string {
  if (ms === null) return t("display.never_opened");
  // 先认时间戳本身：NaN、无穷、以及超出 Date 能表示的范围（损坏的库会给这种值）——
  // 不认就直接说"时间未知"，别算出 "NaN-NaN-NaN"，也别把很远的未来时间说成"今天"
  const date = new Date(ms);
  if (!Number.isFinite(date.getTime())) return t("display.time_unknown");
  // 按**本地日历日**算差（不是毫秒差）：跨零点前后那几十分钟最容易被说错。
  const days = localDaysBetween(ms, now);
  if (days <= 0) return t("display.today");
  if (days === 1) return t("display.yesterday");
  if (days < 30) return t("display.days_ago", { days });
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${date.getFullYear()}-${month}-${day}`;
}

/**
 * 按当前口径把一个数字念出来（**带单位**）：「3.4万 字」/「1200 词」。
 *
 * 单位跟着口径走——逐字报「字」、按词报「词」，别在模板里写死（那是两套口径共用一个句子）。
 */
export function formatCaliberWords(counts: Counts, caliber: string): string {
  return t(countUnitKey(caliber), { count: formatWords(pickCount(counts, caliber)) });
}

/** 只要数字、不要单位的场合（目录树那一列很窄）。 */
export function formatCaliberNumber(counts: Counts, caliber: string): string {
  return formatWords(pickCount(counts, caliber));
}
