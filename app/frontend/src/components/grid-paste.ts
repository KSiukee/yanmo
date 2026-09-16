// 表里那一片格子（从 Excel / WPS / 记事本粘进来的）怎么落到表上：**两个纯函数**。
//
// 拆格（`parsePasteTable`）与落格（`planPaste`）分开，是因为它们的变化理由不一样：
// 前者是"表格软件复制出来的文本长什么样"，后者是"我们这张表现在露着哪几行哪几列"。
//
// 三条拆格规则，都对着真实剪切板：
// 1. 换行分列成行（CRLF / LF 都认）、Tab 分成格；
// 2. **末尾那一个空行不算**——复制一行时文本尾巴常带一个换行，算进去会凭空多一格空的；
// 3. 每格修剪首尾空白（从表里复制出来的格子常带空格）。
//
// 落格只有一条直觉：**锚点就是作者点下去的那一格，往右往下铺**——与在电子表格里粘贴一样。
// 铺不下（列不够、行不够）或者铺到了填不了的格（卷、章名那一列）就**当场说清**，
// 不替作者猜"那这一列跳过吧"：猜错一次，整片都错位。

import type { OutlineCellPayload } from "../api/outline.ts";
import { t } from "../locales/index.ts";
import type { GridColumn } from "./grid.ts";

/**
 * 粘得进去的列：**一句话与四格**。
 *
 * 出场人物是"选出来的"不是"打出来的"，章名 / 伏笔 / 字数是核心算的——
 * 这几列都粘不了。判据写在这里而不是从 `COLUMNS` 的 `editable` 推：
 * "能不能打字"与"能不能整片粘"是两件事（人物那一列能改，但改法是点选）。
 */
const PASTE_COLUMNS: GridColumn[] = ["summary", "pov", "goal", "conflict", "outcome"];

/** 剪切板那一片：一格一个字符串（行 × 列）。 */
export type PasteTable = string[][];

/**
 * 把剪切板文本拆成"行 × 列"。
 *
 * 全是空白的文本给空表（界面据此说"剪贴板里没有可粘的内容"，而不是往格子里写一片空白）。
 */
export function parsePasteTable(text: string): PasteTable {
  const lines = text.replace(/\r\n?/g, "\n").split("\n");
  // 末尾的空行不算一行（复制一行时尾巴上的换行不是"多一行"）
  while (lines.length > 0 && lines[lines.length - 1].trim() === "") lines.pop();
  if (lines.length === 0) return [];
  return lines.map((line) => line.split("\t").map((cell) => cell.trim()));
}

/** 表里现在看得见的一行（粘贴要靠它算"往下铺到哪几段"）。 */
export interface PasteRow {
  node_id: number;
  kind: string;
}

/** 落格失败的原因（界面据此说清"为什么粘不下"）。 */
export type PasteProblem =
  /** 剪切板里没有可粘的内容 */
  | { kind: "empty" }
  /** 第 `column` 列（`name`）是核心算的 / 章名那一列，填不了 */
  | { kind: "column"; column: number; name: string }
  /** 第 `row` 行是卷（分组行），没有可填的格 */
  | { kind: "row"; row: number }
  /** 表尾放不下：还多 `missing` 行 */
  | { kind: "rows"; missing: number }
  /** 表的右边放不下：还多 `missing` 列 */
  | { kind: "columns"; missing: number };

export interface PastePlan {
  cells: OutlineCellPayload[];
}

/**
 * 这一片落在哪几格。
 *
 * `rows` / `columns` 是**界面此刻看得见的那一份**（折叠与筛选之后、列开关之后），
 * `anchor` 是作者点下去的那一格在它们里的下标（行、列各一个）。
 */
export function planPaste(
  table: PasteTable,
  rows: PasteRow[],
  columns: GridColumn[],
  anchor: { row: number; column: number },
): PastePlan | { problem: PasteProblem } {
  if (table.length === 0) return { problem: { kind: "empty" } };

  const width = Math.max(...table.map((line) => line.length));
  if (anchor.row + table.length > rows.length) {
    return { problem: { kind: "rows", missing: anchor.row + table.length - rows.length } };
  }
  if (anchor.column + width > columns.length) {
    return { problem: { kind: "columns", missing: anchor.column + width - columns.length } };
  }

  // 先把"这一片要落的那几行 / 几列"整体过一遍：落了卷或者落在只读列上，整片都不粘
  const targetRows = rows.slice(anchor.row, anchor.row + table.length);
  const targetColumns = columns.slice(anchor.column, anchor.column + width);
  for (const [index, row] of targetRows.entries()) {
    if (row.kind === "volume") return { problem: { kind: "row", row: index + 1 } };
  }
  for (const [index, column] of targetColumns.entries()) {
    if (!PASTE_COLUMNS.includes(column)) {
      return { problem: { kind: "column", column: index + 1, name: t(`grid.col.${column}`) } };
    }
  }

  const cells: OutlineCellPayload[] = [];
  for (const [rowIndex, line] of table.entries()) {
    for (const [columnIndex, value] of line.entries()) {
      cells.push({
        node_id: targetRows[rowIndex].node_id,
        column: targetColumns[columnIndex],
        // 空格也照粘：迁进来的表要跟原表一模一样（空着就是空着，不许悄悄留着旧值）
        value,
      });
    }
  }
  return { cells };
}

/** 粘不下时那句给人看的话（**一处实现**：界面上两处都调它）。 */
export function pasteProblemText(problem: PasteProblem): string {
  switch (problem.kind) {
    case "empty":
      return t("grid.paste.empty");
    case "column":
      return t("grid.paste.readonly", { column: problem.column, name: problem.name });
    case "row":
      return t("grid.paste.on_volume", { row: problem.row });
    case "rows":
      return t("grid.paste.no_rows", { missing: problem.missing });
    case "columns":
      return t("grid.paste.no_columns", { missing: problem.missing });
  }
}
