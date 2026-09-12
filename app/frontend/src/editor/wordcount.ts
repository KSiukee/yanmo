// 字数口径的**显示逻辑**：眼前显示哪一个、点一下换成哪个。
//
// 分寸（别在这里重复核心的事）：
// - **算哪三个数是核心的事**（`text::WordCaliber`），这里一个数都不算，只从三个里挑；
// - **"哪种语言默认哪个口径"也不在这里**——打开一章时壳已经把"落定后的口径"递过来了
//   （`word_caliber`）。在这里再抄一份对照表，两边迟早走偏；
// - 循环顺序与核心 `WordCaliber::ALL` 一致，界面别自己排一套。
//
// 纯函数、不碰 DOM、不 import 界面：可以脱离浏览器单测。

/** 三个口径的稳定代码（与核心一致，存进设置的就是它们）。 */
export type Caliber = "chars" | "chars_no_punct" | "words";
/** 作品语言（与核心 `WorkLanguage` 一致）。 */
export type WorkLanguage = "zh" | "en" | "ja";

/** 点一下的循环顺序——与核心 `WordCaliber::ALL` 同序。 */
export const CALIBERS: readonly Caliber[] = ["chars", "chars_no_punct", "words"];
/** 语言按钮的循环顺序。 */
export const LANGUAGES: readonly WorkLanguage[] = ["zh", "en", "ja"];

/** 认不出来时退回「逐字」：界面永远有个数可显示，不显示空。 */
const FALLBACK_CALIBER: Caliber = "chars";
const FALLBACK_LANGUAGE: WorkLanguage = "zh";

/** 三个数的载体（编辑器每次落盘都会刷新它）。 */
export interface Counts {
  char_count: number;
  chars_no_punct: number;
  word_count: number;
}

/** 口径码 → 口径名（字典键）。字面量写死，动态拼键早晚漏一条没人发现。 */
const CALIBER_LABEL: Record<Caliber, string> = {
  chars: "editor.caliber.chars",
  chars_no_punct: "editor.caliber.chars_no_punct",
  words: "editor.caliber.words",
};

/** 口径 → 计数单位（字典键）：逐字报「字」、按词报「词」。 */
const COUNT_UNIT: Record<Caliber, string> = {
  chars: "editor.word_count",
  chars_no_punct: "editor.word_count",
  words: "editor.count.words",
};

/** 语言码 → 语言名（字典键）。 */
const LANGUAGE_LABEL: Record<WorkLanguage, string> = {
  zh: "editor.language.zh",
  en: "editor.language.en",
  ja: "editor.language.ja",
};

export function asCaliber(value: string | null | undefined): Caliber {
  return CALIBERS.includes(value as Caliber) ? (value as Caliber) : FALLBACK_CALIBER;
}

export function asLanguage(value: string | null | undefined): WorkLanguage {
  return LANGUAGES.includes(value as WorkLanguage) ? (value as WorkLanguage) : FALLBACK_LANGUAGE;
}

/** 点一下换下一个（认不出来的值从第一档开始）。 */
export function nextCaliber(current: string | null | undefined): Caliber {
  const index = CALIBERS.indexOf(asCaliber(current));
  return CALIBERS[(index + 1) % CALIBERS.length];
}

/** 点一下换下一个语言。 */
export function nextLanguage(current: string | null | undefined): WorkLanguage {
  const index = LANGUAGES.indexOf(asLanguage(current));
  return LANGUAGES[(index + 1) % LANGUAGES.length];
}

/** 从三个数里取当前口径那一个。 */
export function pickCount(counts: Counts, caliber: string | null | undefined): number {
  switch (asCaliber(caliber)) {
    case "chars_no_punct":
      return counts.chars_no_punct;
    case "words":
      return counts.word_count;
    default:
      return counts.char_count;
  }
}

export function caliberLabelKey(caliber: string | null | undefined): string {
  return CALIBER_LABEL[asCaliber(caliber)];
}

export function countUnitKey(caliber: string | null | undefined): string {
  return COUNT_UNIT[asCaliber(caliber)];
}

export function languageLabelKey(language: string | null | undefined): string {
  return LANGUAGE_LABEL[asLanguage(language)];
}
