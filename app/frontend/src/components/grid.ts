// 大纲表的**纯展示逻辑**：有哪些列、哪些行要露、哪一格该提醒、键盘往哪儿走。
//
// 单独成文件的原因与 `owner.ts` 那一族同一条：这些都是纯函数，能单测；
// 组件只管把它们摆上去。文案一律从字典取（`grid.*`）——界面文案只有那一处来源。

import { t } from "../locales/index.ts";

/** 表里的一列（稳定键；列名在字典 `grid.col.*` 里）。 */
export type GridColumn =
  | "title"
  | "summary"
  | "pov"
  | "goal"
  | "conflict"
  | "outcome"
  | "foreshadow"
  | "words";

export interface ColumnSpec {
  key: GridColumn;
  /** 这一列能不能改（章名与字数是核心算的；伏笔要改去伏笔账本） */
  editable: boolean;
}

/** 全部列（顺序就是表里的顺序；用户勾选显示哪几列）。 */
export const COLUMNS: ColumnSpec[] = [
  { key: "title", editable: false },
  { key: "summary", editable: true },
  { key: "pov", editable: true },
  { key: "goal", editable: true },
  { key: "conflict", editable: true },
  { key: "outcome", editable: true },
  { key: "foreshadow", editable: false },
  { key: "words", editable: false },
];

/** 默认露哪几列：章名 + 一句话 + 四格（伏笔与字数按需勾出来）。 */
export const DEFAULT_COLUMNS: GridColumn[] = ["title", "summary", "pov", "goal", "conflict", "outcome"];

/** 四格那一族（数据列的真身：它们在 `fields` 里，不在行对象顶层）。 */
export const FIELD_COLUMNS = ["pov", "goal", "conflict", "outcome"] as const;

/**
 * 这一格能不能填。
 *
 * **卷不算一格**：它是分组行（表里整行铺开当小标题，不占格）——所以对卷来说
 * 哪一列都填不了，Enter 往下走也要跳过它。
 */
export function cellEditable(spec: ColumnSpec, row: { kind: string }): boolean {
  if (!spec.editable) return false;
  return row.kind !== "volume";
}

/** 一格里现在是什么值。 */
export function cellValue(row: Record<string, unknown>, column: GridColumn): string {
  if (column === "title") return String(row.title ?? "");
  if (column === "summary") return String(row.summary ?? "");
  if (column === "words") return String(row.word_count ?? "");
  const fields = (row.fields ?? {}) as Record<string, string>;
  return String(fields[column] ?? "");
}

/** 四格里哪几格还是空的（**判据只有一条**：修剪后是不是空的——与核心同一条口径）。 */
export function emptyFields(row: { fields: Record<string, string> }): string[] {
  return FIELD_COLUMNS.filter((field) => (row.fields?.[field] ?? "").trim() === "");
}

/**
 * 这一行要不要作者再管一下：**没写一句话**，或者四格**填了一半**。
 *
 * "四格一个字都没填"不算——那是"还没打算填"，一本没规划过的书会在表里全亮，
 * 把真正该看的那几行埋掉（与体检那条规则同一条口径）。
 */
export function needsAttention(row: {
  fields: Record<string, string>;
  summary: string;
  kind: string;
}): boolean {
  if (row.kind === "volume") return false;
  const empty = emptyFields(row);
  if (empty.length === FIELD_COLUMNS.length) return row.summary.trim() === "";
  return empty.length > 0 || row.summary.trim() === "";
}

/** 伏笔那一列念什么：「埋 1 / 收 2」；一条都没有就不摆字。 */
export function foreshadowText(row: { planted_open: number; collected: number }): string {
  const parts: string[] = [];
  if (row.planted_open > 0) parts.push(t("grid.foreshadow.planted", { count: row.planted_open }));
  if (row.collected > 0) parts.push(t("grid.foreshadow.collected", { count: row.collected }));
  return parts.join(" / ");
}

export interface GridRow {
  node_id: number;
  parent_id: number | null;
  kind: string;
  depth: number;
  fields: Record<string, string>;
  summary: string;
  planted_open: number;
  collected: number;
}

/** 表里要摆的行：先按折叠状态剔掉收起的分组，再按"只看没填的"筛。 */
export function visibleRows<T extends GridRow>(
  rows: T[],
  options: { collapsed: Set<number>; onlyUnfilled: boolean },
): T[] {
  const out: T[] = [];
  let hiddenUnder: number | null = null;
  for (const row of rows) {
    // 收起来的卷：它下面（更深）的行全不摆，直到回到同层或更浅
    if (hiddenUnder !== null) {
      if (row.depth > (rows.find((item) => item.node_id === hiddenUnder)?.depth ?? 0)) continue;
      hiddenUnder = null;
    }
    if (row.kind === "volume" && options.collapsed.has(row.node_id)) {
      hiddenUnder = row.node_id;
    }
    out.push(row);
  }
  if (!options.onlyUnfilled) return out;
  // 只看没填的：卷留着（分组头），别的按 needsAttention
  return out.filter((row) => row.kind === "volume" || needsAttention(row));
}

/**
 * 从这一行往下（或往上）找**下一个能填这一列**的行。
 *
 * 表里卷是分组行、四格对它没意义——按 Enter 往下填的时候要跳过它，
 * 不然会停在一个填不进去的格子上（"按了没反应"是最容易被骂的一类手感）。
 */
export function nextFillableRow<T extends GridRow>(
  rows: T[],
  from: number,
  column: GridColumn,
  step: 1 | -1,
): number | null {
  const spec = COLUMNS.find((item) => item.key === column);
  if (!spec) return null;
  for (let index = from + step; index >= 0 && index < rows.length; index += step) {
    if (cellEditable(spec, rows[index])) return index;
  }
  return null;
}

/** 这一列的宽度（写死的像素：表要横着排得下，列宽就别让内容自己撑）。 */
export function columnWidth(column: GridColumn): string {
  if (column === "title") return "12rem";
  if (column === "summary" || column === "conflict" || column === "outcome") return "16rem";
  if (column === "foreshadow") return "7rem";
  if (column === "words") return "5rem";
  return "10rem";
}
