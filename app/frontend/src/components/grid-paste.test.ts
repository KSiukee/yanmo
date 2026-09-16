// 整片粘贴的纯逻辑验收：**拆格**与**落点**。
//
// 这一层没有 DOM：组件只管把剪切板文本交给它们、把落点交给核心。

import { test } from "node:test";
import assert from "node:assert/strict";

import { parsePasteTable, pasteProblemText, planPaste, type PasteRow } from "./grid-paste.ts";
import type { GridColumn } from "./grid.ts";

const chapter = (node_id: number): PasteRow => ({ node_id, kind: "chapter" });
const ALL: GridColumn[] = ["title", "summary", "cast", "pov", "goal", "conflict", "outcome"];
/** 没勾出场人物那一列时的表（列是作者自己勾的，粘贴要按**当前露着的那几列**算落点）。 */
const NO_CAST: GridColumn[] = ["title", "summary", "pov", "goal", "conflict", "outcome"];

test("拆格：换行分列成行、Tab 分成格、每格修剪空白", () => {
  assert.deepEqual(parsePasteTable("他进城\t陆文\n 拿到账本 \t老张 "), [
    ["他进城", "陆文"],
    ["拿到账本", "老张"],
  ]);
});

test("拆格：CRLF 也认；末尾那一个空行不算一行；全空白给空表", () => {
  assert.deepEqual(parsePasteTable("a\r\nb\r\n"), [["a"], ["b"]], "尾巴上的换行不是多一行");
  assert.deepEqual(parsePasteTable("   \n\n"), [], "全是空白＝没有可粘的内容");
  assert.deepEqual(parsePasteTable(""), []);
  // 中间的空行**算一行**（它是"这一格留空"，不是尾巴）
  assert.deepEqual(parsePasteTable("a\n\nb"), [["a"], [""], ["b"]]);
});

test("落格：锚点是点下去的那一格，往右往下铺", () => {
  const rows = [chapter(1), chapter(2), chapter(3)];
  const plan = planPaste([["一句话一", "视角一"], ["一句话二", "视角二"]], rows, NO_CAST, {
    row: 1,
    column: 1,
  });
  assert.ok("cells" in plan);
  assert.deepEqual(plan.cells, [
    { node_id: 2, column: "summary", value: "一句话一" },
    { node_id: 2, column: "pov", value: "视角一" },
    { node_id: 3, column: "summary", value: "一句话二" },
    { node_id: 3, column: "pov", value: "视角二" },
  ]);
});

test("落格：单列粘贴（Excel 里最常见的那一下）", () => {
  const rows = [chapter(1), chapter(2), chapter(3)];
  const plan = planPaste([["甲"], ["乙"], ["丙"]], rows, ALL, { row: 0, column: 3 });
  assert.ok("cells" in plan);
  assert.deepEqual(
    plan.cells.map((cell) => [cell.node_id, cell.column, cell.value]),
    [
      [1, "pov", "甲"],
      [2, "pov", "乙"],
      [3, "pov", "丙"],
    ],
  );
});

test("落格：不是打字填的列**原地跳过**，别的列不错位", () => {
  // 从 Excel 迁一整张表时，出场人物那一列几乎一定在选中的区域里——
  // 为它把整片退回去等于"一键迁入"永远迁不进来
  const plan = planPaste([["他进城", "陆文、老张", "陆文"]], [chapter(1)], ALL, {
    row: 0,
    column: 1,
  });
  assert.ok("cells" in plan);
  assert.deepEqual(plan.skipped, ["cast"]);
  assert.deepEqual(plan.cells, [
    { node_id: 1, column: "summary", value: "他进城" },
    { node_id: 1, column: "pov", value: "陆文" },
  ]);
});

test("落格：整片都落在非打字的列上 → 一个字都没写", () => {
  const plan = planPaste([["陆文"]], [chapter(1)], ALL, { row: 0, column: 2 });
  assert.ok("cells" in plan);
  assert.deepEqual(plan.cells, []);
  assert.deepEqual(plan.skipped, ["cast"]);
});

test("落格：空格也照粘（迁进来的表要跟原表对得上）", () => {
  const plan = planPaste([[""], ["他进城"]], [chapter(1), chapter(2)], ALL, { row: 0, column: 1 });
  assert.ok("cells" in plan);
  assert.deepEqual(plan.cells[0], { node_id: 1, column: "summary", value: "" });
});

test("落格：粘不下时说清是哪一种，不给半片", () => {
  const rows = [chapter(1), chapter(2)];
  // 行不够
  const tooManyRows = planPaste([["a"], ["b"], ["c"]], rows, ALL, { row: 0, column: 1 });
  assert.ok("problem" in tooManyRows);
  assert.deepEqual(tooManyRows.problem, { kind: "rows", missing: 1 });

  // 列不够（锚点在最后一列）
  const tooManyColumns = planPaste([["a", "b"]], rows, ALL, { row: 0, column: ALL.length - 1 });
  assert.ok("problem" in tooManyColumns);
  assert.deepEqual(tooManyColumns.problem, { kind: "columns", missing: 1 });

  // 落在卷上（分组行没有可填的格）
  const onVolume = planPaste([["x"]], [{ node_id: 1, kind: "volume" }, chapter(2)], ALL, {
    row: 0,
    column: 1,
  });
  assert.ok("problem" in onVolume);
  assert.deepEqual(onVolume.problem, { kind: "row", row: 1 });

  // 空剪切板
  const empty = planPaste([], rows, ALL, { row: 0, column: 1 });
  assert.ok("problem" in empty);
  assert.deepEqual(empty.problem, { kind: "empty" });
});

test("粘不下时那句话把数字填进去了（不留 {missing} 这种花括号）", () => {
  for (const text of [
    pasteProblemText({ kind: "empty" }),
    pasteProblemText({ kind: "row", row: 1 }),
    pasteProblemText({ kind: "rows", missing: 3 }),
    pasteProblemText({ kind: "columns", missing: 2 }),
  ]) {
    assert.ok(text.length > 0);
    assert.ok(!text.includes("{"), `文案里不该留占位符：${text}`);
  }
});
