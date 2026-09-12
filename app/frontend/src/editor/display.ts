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

/** 最近打开时间给人看：今天 / 昨天 / N 天前 / 具体日期；从没打开过就直说。 */
export function formatWhen(ms: number | null, now: number = Date.now()): string {
  if (ms === null) return t("display.never_opened");
  // 先认时间戳本身：NaN、无穷、以及超出 Date 能表示的范围（损坏的库会给这种值）——
  // 不认就直接说"时间未知"，别算出 "NaN-NaN-NaN"，也别把很远的未来时间说成"今天"
  const date = new Date(ms);
  if (!Number.isFinite(date.getTime())) return t("display.time_unknown");
  const days = Math.floor((now - ms) / 86_400_000);
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
