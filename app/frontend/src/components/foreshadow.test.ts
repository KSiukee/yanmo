// 伏笔那一屏的纯逻辑验收：状态称呼、筛选项、章的名分、下一步能走哪儿。
//
// 状态机的**合法性**在核心（那边有单测）；这里只盯"界面摆什么"。

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  anchorLabel,
  chapterLabel,
  filterOptions,
  nextStates,
  stateLabel,
  visibleItems,
  whereLabel,
} from "./foreshadow.ts";
import type { ChapterRef, Foreshadow, ForeshadowBoard } from "../api/foreshadow.ts";

const chapters: ChapterRef[] = [
  { id: 11, title: "第1章", index: 1 },
  { id: 12, title: "第2章", index: 2 },
];

function item(id: number, state: Foreshadow["state"], planted: number | null, collected: number | null = null): Foreshadow {
  return {
    id,
    work_id: 1,
    body: `第 ${id} 条线头`,
    planted_node: planted,
    collected_node: collected,
    state,
    note: "",
    created_at: 0,
    updated_at: 0,
  };
}

const board: ForeshadowBoard = {
  items: [item(1, "planted", 11), item(2, "collected", 11, 12), item(3, "dropped", null)],
  planted: 1,
  collected: 1,
  dropped: 1,
  chapters,
};

test("状态称呼走字典；字典里没有的照原样露出来", () => {
  assert.equal(stateLabel("planted"), "埋着");
  assert.equal(stateLabel("collected"), "收了");
  assert.equal(stateLabel("dropped"), "不写了");
  assert.equal(stateLabel("maybe"), "maybe");
});

test("筛选项：全部在最前，三个状态都摆（0 的也摆，好知道是 0）", () => {
  assert.deepEqual(filterOptions(board), [
    { state: "all", count: 3 },
    { state: "planted", count: 1 },
    { state: "collected", count: 1 },
    { state: "dropped", count: 1 },
  ]);
  assert.deepEqual(filterOptions(null), [{ state: "all", count: 0 }]);
});

test("筛选：all 原样，其余按状态挑", () => {
  assert.equal(visibleItems(board.items, "all").length, 3);
  assert.deepEqual(visibleItems(board.items, "planted").map((one) => one.id), [1]);
  assert.deepEqual(visibleItems(board.items, "dropped").map((one) => one.id), [3]);
});

test("章的名分：查得到就说第几章，查不到不编号（不编一个号出来）", () => {
  assert.equal(chapterLabel(chapters, 11), "第 1 章");
  assert.equal(chapterLabel(chapters, 12), "第 2 章");
  assert.equal(chapterLabel(chapters, null), "", "没记就是空");
  assert.equal(chapterLabel(chapters, 99), "", "锚点在别处 / 刚被删：不编号");

  assert.equal(anchorLabel(chapters, 11, "planted"), "埋于第 1 章");
  assert.equal(anchorLabel(chapters, 12, "collected"), "收于第 2 章");
  assert.equal(anchorLabel(chapters, 99, "planted"), "");
  assert.equal(whereLabel(chapters, item(1, "planted", 11)), "埋于第 1 章");
  assert.equal(whereLabel(chapters, item(3, "dropped", null)), "", "没记埋点：这一截不摆");
});

test("下一步能走哪儿：埋着给「收了 / 不写了」，别的都能回到「埋着」", () => {
  assert.deepEqual(nextStates("planted"), ["collected", "dropped"]);
  assert.deepEqual(nextStates("collected"), ["planted"]);
  assert.deepEqual(nextStates("dropped"), ["planted"]);
});
