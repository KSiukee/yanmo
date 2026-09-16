// 创作流面板的纯逻辑验收：筛选项、筛选、种类称呼、是不是这一章写的。
//
// 这一层没有 DOM，也不需要后端——它只管"摆什么"（"会发生什么"在核心那一层）。

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  anchoredToChapter,
  canCarryStoryTime,
  filterOptions,
  JOTTED_KINDS,
  kindLabel,
  parseStoryOrder,
  sourceBadge,
  storyLabel,
  storyOrderText,
  totalCount,
  visibleFragments,
  WRITABLE_KINDS,
} from "./creator.ts";
import type { CreatorBoard, Fragment } from "../api/fragment.ts";

function fragment(id: number, kind: Fragment["kind"], body = "一句"): Fragment {
  return {
    id,
    work_id: 1,
    kind,
    body,
    source: "typed",
    anchors: [],
    derived_from: null,
    created_at: 1_000 + id,
    story_time: "",
    story_order: null,
    flashback: false,
  };
}

const board: CreatorBoard = {
  fragments: [
    fragment(1, "idea", "一个念头"),
    fragment(2, "event", "他走进来"),
    fragment(3, "idea", "又一个念头"),
  ],
  counts: [
    { kind: "idea", count: 2 },
    { kind: "event", count: 1 },
    { kind: "dictation", count: 0 },
    { kind: "question", count: 7 },
    { kind: "answer", count: 3 },
  ],
};

test("筛选项：全部在最前，空种类不摆出来，也不把问题/答案算进来", () => {
  assert.deepEqual(filterOptions(board), [
    { kind: "all", count: 3 },
    { kind: "idea", count: 2 },
    { kind: "event", count: 1 },
  ]);
  // 「全部」与各档对得上：它数的是同一种类口径（不是当前这一屏）
  assert.equal(totalCount(board), 3);
});

test("没有书（board 为 null）时只剩「全部 0」", () => {
  assert.deepEqual(filterOptions(null), [{ kind: "all", count: 0 }]);
  assert.equal(totalCount(null), 0);
});

test("筛选：all 原样，其余按种类挑", () => {
  assert.equal(visibleFragments(board.fragments, "all").length, 3);
  assert.deepEqual(
    visibleFragments(board.fragments, "idea").map((item) => item.id),
    [1, 3],
  );
  assert.deepEqual(visibleFragments(board.fragments, "event").map((item) => item.id), [2]);
  // 核心那边先长了新种类时也不炸：查不到就是空
  assert.deepEqual(visibleFragments(board.fragments, "dictation"), []);
});

test("种类称呼走字典；字典里没有的照原样露出来（别静默说成别的）", () => {
  assert.equal(kindLabel("idea"), "灵感");
  assert.equal(kindLabel("event"), "事件");
  assert.equal(kindLabel("dictation"), "口述");
  assert.equal(kindLabel("memo"), "memo");
});

test("只有不是手打的才值得标一句", () => {
  assert.equal(sourceBadge("typed"), "");
  assert.equal(sourceBadge("voice"), "口述");
  assert.equal(sourceBadge("mixed"), "口述后改过");
});

test("是不是在这一章写下的：认 chapter:<章 id>", () => {
  const item = { ...fragment(1, "idea"), anchors: ["chapter:7", "chapter:9"] };
  assert.equal(anchoredToChapter(item.anchors, 7), true);
  assert.equal(anchoredToChapter(item.anchors, 9), true);
  // 没打开着任何一章时不该说"就是这一章"
  assert.equal(anchoredToChapter(item.anchors, null), false);
  assert.equal(anchoredToChapter(item.anchors, 8), false);
});

test("能记的种类是随手记那一组的子集（口述不靠键盘记）", () => {
  for (const kind of WRITABLE_KINDS) {
    assert.ok(JOTTED_KINDS.includes(kind), `${kind} 不在随手记那一组里`);
  }
  assert.deepEqual(WRITABLE_KINDS, ["idea", "event"]);
});

test("故事时间的排序值：文本 → 数字（空着就是没填，写不进数字也当没填）", () => {
  assert.equal(parseStoryOrder(""), null);
  assert.equal(parseStoryOrder("   "), null);
  assert.equal(parseStoryOrder("12"), 12);
  assert.equal(parseStoryOrder(" 12.7 "), 12, "小数截断（天数是整数）");
  assert.equal(parseStoryOrder("-3"), -3, "负号也认（作者自己定的口径）");
  assert.equal(parseStoryOrder("第三天"), null, "写不进数字 = 没填，不猜");
  assert.equal(storyOrderText(null), "");
  assert.equal(storyOrderText(12), "12");
});

test("事件身上那个小标：有自由文本用它，没有就说第 N 天，都没有就不摆", () => {
  assert.equal(storyLabel({ story_time: "承平三年·春", story_order: 12 }), "承平三年·春");
  assert.equal(storyLabel({ story_time: "  ", story_order: 12 }), "第 12 天");
  assert.equal(storyLabel({ story_time: "", story_order: null }), "");
});

test("只有事件才有故事时间（别的种类不摆那一栏）", () => {
  assert.equal(canCarryStoryTime("event"), true);
  assert.equal(canCarryStoryTime("idea"), false);
  assert.equal(canCarryStoryTime("dictation"), false);
});
