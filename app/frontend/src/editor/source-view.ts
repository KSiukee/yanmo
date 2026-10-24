// 状态栏那一行「这一章的字从哪来」的**显示逻辑**（纯函数，不碰 DOM）。
//
// 分寸：
// - **数值全部来自核心**（`SourceBreakdown`），这里只挑口径、排序、决定露哪几档；
// - **只露非「键入」的档**：打字是常态，不需要一行字提醒作者"我正在打字"；
//   真正要看见的是"其中有多少不是打出来的"——粘贴 / 模块 / 未记录。
// - 顺序照核心 `EditOrigin::ALL`：界面不另排一套（两套顺序迟早对不上）。

import type { SourceBreakdown, SourceShare } from "../api/source.ts";
import { asCaliber, type Caliber } from "./wordcount.ts";

/** 一行读数：哪一档、当前口径下多少字。 */
export interface SourceLine {
  source: string;
  count: number;
}

/** 与核心 `EditOrigin::ALL` 同序。 */
const ORDER = ["typed", "pasted", "module", "voice", "system", "mixed", "unknown"] as const;

/** 键盘打出来的那一档（不单独露；它是"其余"的底色）。 */
const TYPED = "typed";

function countOf(share: SourceShare, caliber: Caliber): number {
  switch (caliber) {
    case "chars_no_punct":
      return share.chars_no_punct;
    case "words":
      return share.word_count;
    default:
      return share.char_count;
  }
}

function rank(source: string): number {
  const at = ORDER.indexOf(source as (typeof ORDER)[number]);
  return at === -1 ? ORDER.length : at;
}

/** 全部非零档（含键入），按固定顺序——给悬停提示用。 */
export function sourceRows(breakdown: SourceBreakdown, caliber: string | null | undefined): SourceLine[] {
  const selected = asCaliber(caliber);
  return breakdown.shares
    .map((share) => ({ source: share.source, count: countOf(share, selected) }))
    .filter((line) => line.count > 0)
    .sort((left, right) => rank(left.source) - rank(right.source));
}

/** 状态栏里露出来的那几档：**非键入、非零**。全键入时返回空（那一行不出现）。 */
export function sourceHighlights(
  breakdown: SourceBreakdown,
  caliber: string | null | undefined,
): SourceLine[] {
  return sourceRows(breakdown, caliber).filter((line) => line.source !== TYPED);
}

/** 字典键：`editor.source.<码>`。认不出的码给"其他"（不让界面露出一串英文码）。 */
export function sourceLabelKey(source: string): string {
  return ORDER.includes(source as (typeof ORDER)[number])
    ? `editor.source.${source}`
    : "editor.source.other";
}
