// 「五线」那一格的纯逻辑验收：徽标怎么摆、勾一下交出去的是哪一份、按节点怎么并。
//
// 这一层没有 DOM：组件（`OutlineLinesCell.vue` / `OutlineLinesPicker.vue`）只管把它们摆上去。
// 与核心那一条口径对着看：`crates/yanmo-core/src/model/story_line.rs`（五条闭集、稳定码、
// "空＝还没标"）。

import { test } from "node:test";
import assert from "node:assert/strict";

import { t } from "../locales/index.ts";
import {
  attachLines,
  badgeTexts,
  lineName,
  markedLines,
  sameLines,
  STORY_LINES,
  toggleLine,
  unmarked,
} from "./outline-lines.ts";

test("五条线就是五条，码与库里的稳定码一字不差（改了等于把作者的标注读成别人的意思）", () => {
  assert.deepEqual([...STORY_LINES], ["main", "sub", "romance", "suspense", "growth"]);
  for (const code of STORY_LINES) {
    assert.notEqual(t(`grid.lines.short.${code}`), `grid.lines.short.${code}`, `${code} 的简写要在字典里`);
    assert.notEqual(t(`grid.lines.name.${code}`), `grid.lines.name.${code}`, `${code} 的全名要在字典里`);
  }
  assert.equal(lineName("romance"), "感情线");
  assert.equal(t("grid.col.lines"), "五线", "列头一个字");
});

test("徽标：标 3 条 → 三个徽标，按固定顺序（与输入顺序无关）", () => {
  assert.deepEqual(badgeTexts(["growth", "main", "romance"]), ["主", "情", "长"]);
  assert.deepEqual(badgeTexts(["main"]), ["主"]);
  assert.deepEqual(markedLines(["growth", "main", "main"]), ["main", "growth"], "去重 + 固定序");
  assert.deepEqual(markedLines(["plot"]), [], "认不出的码丢掉（界面不替核心乱猜）");
});

test("空 → 未标：不是空白，也不是「0 条」（空是还没标，不是没推进）", () => {
  assert.equal(t("grid.lines.none"), "未标");
  assert.equal(unmarked([]), true);
  assert.equal(unmarked(undefined), true);
  assert.equal(unmarked(["plot"]), true, "认不出的码等于没标：那几条不往格子上摆");
  assert.equal(unmarked(["main"]), false);
  assert.deepEqual(badgeTexts([]), [], "没标过的格子摆徽标的地方是空的（那一格改摆「未标」）");
});

test("勾 / 取消：交出去的是**整份清单**——取消一条＝少一条，不是再加一条", () => {
  assert.deepEqual(toggleLine(["main", "romance"], "growth"), ["main", "romance", "growth"]);
  assert.deepEqual(toggleLine(["main", "romance", "growth"], "romance"), ["main", "growth"]);
  assert.deepEqual(toggleLine([], "suspense"), ["suspense"]);
  assert.deepEqual(toggleLine(["romance", "main"], "sub"), ["main", "sub", "romance"], "载荷排回固定序");
  assert.deepEqual(toggleLine(["main"], "main"), [], "取消最后一条＝清空（整份覆盖下这是正经操作）");
});

test("一样就不写：先排成固定序再比（顺序不同不算改过）", () => {
  assert.equal(sameLines(["main", "sub"], ["sub", "main"]), true);
  assert.equal(sameLines(["main"], ["main", "sub"]), false);
  assert.equal(sameLines([], undefined), true);
});

test("按节点 id 把标注并到行上：没标过的段补空清单，别的列原样留着", () => {
  const rows = [
    { node_id: 1, title: "第1章" },
    { node_id: 2, title: "第2章" },
    { node_id: 3, title: "第3章" },
  ];
  const merged = attachLines(rows, [
    { node_id: 2, lines: ["sub"] },
    { node_id: 3, lines: [] },
  ]);
  assert.deepEqual(merged.find((row) => row.node_id === 2)?.lines, ["sub"]);
  assert.deepEqual(merged.find((row) => row.node_id === 1)?.lines, [], "没标过的段补空清单");
  assert.equal(merged[0].title, "第1章", "别的列原样留着");
  assert.equal(merged.length, rows.length, "一行都不多、一行都不少");
});

test("全名五个码各有一句：码 → 中文名（叩问面板翻的就是这一份，不是另抄的）", () => {
  assert.deepEqual(
    STORY_LINES.map((code) => lineName(code)),
    ["主线", "副线", "感情线", "悬念线", "成长线"],
  );
});
