// 拖动落点的纯逻辑验收：**上 / 中 / 下三段各自该落成什么**。
//
// 这一层盯的就是那个"差一格"：核心是"先把自己摘出去，再插到第 index 位"，
// 所以往下拖要减掉自己那一格。目录树原来那版没减，往下拖一格会掉两格。

import { test } from "node:test";
import assert from "node:assert/strict";

import { dropPlan, inSubtree, type DropRow } from "./drop-plan.ts";

const rows: DropRow[] = [
  { id: 1, parent_id: null }, // 卷 1
  { id: 11, parent_id: 1 }, // 章 1
  { id: 12, parent_id: 1 }, // 章 2
  { id: 13, parent_id: 1 }, // 章 3
  { id: 2, parent_id: null }, // 卷 2
  { id: 21, parent_id: 2 },
];

test("往下拖一格就是往下拖一格（不许掉两格）", () => {
  // 第 1 章拖到第 2 章后面 → 第 1 章该落在第 2 章的位置
  assert.deepEqual(dropPlan(rows, 11, 12, "after"), { parent_id: 1, index: 1 });
  // 第 1 章拖到第 2 章前面 → 原地不动（它就是第 0 位）
  assert.deepEqual(dropPlan(rows, 11, 12, "before"), { parent_id: 1, index: 0 });
  // 第 3 章拖到第 1 章前面 → 第 0 位
  assert.deepEqual(dropPlan(rows, 13, 11, "before"), { parent_id: 1, index: 0 });
  // 第 3 章拖到第 1 章后面 → 第 1 位
  assert.deepEqual(dropPlan(rows, 13, 11, "after"), { parent_id: 1, index: 1 });
});

test("跨卷：落点跟着目标那一行的父级走", () => {
  // 卷 2 里的章拖到第 3 章后面 → 落进卷 1
  assert.deepEqual(dropPlan(rows, 21, 13, "after"), { parent_id: 1, index: 3 });
  // 第 2 章排到卷 2 前面（根级，紧跟卷 1 之后）
  assert.deepEqual(dropPlan(rows, 12, 2, "before"), { parent_id: null, index: 1 });
});

test("放进卷里：追加到那一层末尾（越界由核心夹）", () => {
  assert.deepEqual(dropPlan(rows, 12, 2, "inside"), {
    parent_id: 2,
    index: Number.MAX_SAFE_INTEGER,
  });
});

test("不许拖进自己那一支（会成环）：整个不给落点", () => {
  assert.equal(dropPlan(rows, 1, 12, "after"), null, "卷排到自己章后面 = 成环");
  assert.equal(dropPlan(rows, 1, 1, "inside"), null, "拖到自己身上");
  assert.equal(dropPlan(rows, 11, 11, "before"), null);
  assert.equal(dropPlan(rows, 999, 11, "before"), null, "拖的那一行不在表里");
  assert.equal(dropPlan(rows, 11, 999, "before"), null, "目标行不在表里");
});

test("父链走法：自己在自己那一支里算得对，坏数据也不会转不完", () => {
  assert.equal(inSubtree(rows, 12, 1), true);
  assert.equal(inSubtree(rows, 1, 12), false);
  assert.equal(inSubtree(rows, 1, 1), true, "自己算在自己那一支里");
  // 成环的坏数据：走到步数上限就停，不挂住
  const looped: DropRow[] = [
    { id: 1, parent_id: 2 },
    { id: 2, parent_id: 1 },
  ];
  assert.equal(inSubtree(looped, 1, 99), false);
});
