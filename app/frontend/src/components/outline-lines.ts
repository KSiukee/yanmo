// 「五线」那一格的**纯逻辑**：哪几条线、按什么顺序摆、勾一下之后交出去的整份载荷长什么样。
//
// 单独成文件的原因与 `grid.ts` 那一族同一条：这些都是纯函数，能单测；组件只管把它们摆上去。
// 文案一律从字典取（`grid.lines.*`）——界面文案只有那一处来源。
//
// 三条口径（与核心 `model::story_line` 逐条对齐）：
// 1. **闭集**：五条就是五条，码稳定（`main` / `sub` / `romance` / `suspense` / `growth`）；
// 2. **顺序固定**：摆出来与交出去都按 `STORY_LINES` 这一份顺序，与库里那几行的顺序无关
//    （库里读回来也是同一份顺序，见 `store::lines` 的稳定序）；
// 3. **空是"还没标"**：空数组只说明"还没说"，**不是**"没推进"——所以那一格摆「未标」，
//    不摆"0 条"，也不替作者下结论（这条写在核心 `model::story_line` 的口径里）。
import type { NodeLinesDto } from "../api/outline.ts";
import { t } from "../locales/index.ts";

/** 五条线的稳定码（顺序就是勾选框与徽标的顺序）。 */
export const STORY_LINES = ["main", "sub", "romance", "suspense", "growth"] as const;

export type StoryLineCode = (typeof STORY_LINES)[number];

/** 这一段标了哪几条（**固定顺序**、去重；认不出的码丢掉——界面不替核心乱猜）。 */
export function markedLines(lines: readonly string[] | undefined): StoryLineCode[] {
  return STORY_LINES.filter((code) => (lines ?? []).includes(code));
}

/** 一条都没标：那一格摆「未标」（不是空白，也不是"0 条"）。 */
export function unmarked(lines: readonly string[] | undefined): boolean {
  return markedLines(lines).length === 0;
}

/** 徽标的字（`主` / `副` / `情` / `悬` / `长`），按固定顺序——就是格子上的那排小徽标。 */
export function badgeTexts(lines: readonly string[] | undefined): string[] {
  return markedLines(lines).map((code) => t(`grid.lines.short.${code}`));
}

/** 勾选框旁边那一行全名（`主线` / `副线` / `感情线` / `悬念线` / `成长线`）。 */
export function lineName(code: StoryLineCode): string {
  return t(`grid.lines.name.${code}`);
}

/**
 * 勾上 / 取消一条，返回**这一段的完整清单**。
 *
 * 交出去的就是整份（核心那条写是整份覆盖）：取消勾＝少一条，不是"再加一条"。
 * 顺序排回固定序，于是"点两下的顺序不同"不会让载荷不同。
 */
export function toggleLine(
  lines: readonly string[] | undefined,
  code: StoryLineCode,
): StoryLineCode[] {
  const marked = markedLines(lines);
  const kept = marked.includes(code) ? marked.filter((item) => item !== code) : [...marked, code];
  return STORY_LINES.filter((item) => kept.includes(item));
}

/** 两份清单是不是同一份（**先排成固定序再比**：顺序不同不算改过）。 */
export function sameLines(
  left: readonly string[] | undefined,
  right: readonly string[] | undefined,
): boolean {
  const a = markedLines(left);
  const b = markedLines(right);
  return a.length === b.length && a.every((code, index) => code === b[index]);
}

/**
 * 把 `node_lines` 那一份**按节点 id 并到大纲表的行上**（没标过的段补一个空清单）。
 *
 * 两处读的是两条链（表与标注分开，见 `api/outline.ts`），合并只在这一处做：
 * 组件里各拼一遍，迟早会出现"这一格与那一格对不上"。
 */
export function attachLines<T extends { node_id: number }>(
  rows: readonly T[],
  list: readonly NodeLinesDto[],
): (T & { lines: string[] })[] {
  const byNode = new Map(list.map((item) => [item.node_id, item.lines]));
  return rows.map((row) => ({ ...row, lines: byNode.get(row.node_id) ?? [] }));
}

