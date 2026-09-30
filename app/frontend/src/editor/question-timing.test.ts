// 推的时机与「它说的那一章」必须配对——这条曾经真错过一次。
//
// 出处：加了核心的「空章不问」判据之后，「写完一章 → 切到空章」这条最常见的路**一次都问不出来**——
// 因为切章那一刻 `chapter_done` 说的明明是刚写完的那一章，壳体却把**新到的空章**递了下去，
// 于是核心按"你在空章里说这一章写完了"把它拦掉了。这条测试盯的就是这个配对。
import { test } from "node:test";
import assert from "node:assert/strict";

import { switchPush } from "./question-timing.ts";

test("刚写完一章就走：说的是**刚离开**的那一章，不是切过去的空章", () => {
  const intent = switchPush({ node: 7, chars: 800 }, { node: 8, chars: 0 });
  assert.deepEqual(intent, { reason: "chapter_done", about: 7 }, "问的是第 7 章（刚写完那章）");
});

test("到了一章还没写字的地方：说的是**新到**的那一章", () => {
  // 离开的那一章本来也是空的（还没动笔）⇒ 不是"写完一章"，而是"开新章"
  const intent = switchPush({ node: 7, chars: 0 }, { node: 8, chars: 0 });
  assert.deepEqual(intent, { reason: "new_chapter", about: 8 });
  // 从"没有打开的章"进到空章，同样算开新章
  assert.deepEqual(switchPush({ node: null, chars: 0 }, { node: 8, chars: 0 }), {
    reason: "new_chapter",
    about: 8,
  });
});

test("同一章里字数变了不算切章；切到有正文的章也不问", () => {
  assert.equal(switchPush({ node: 7, chars: 100 }, { node: 7, chars: 300 }), null);
  assert.equal(switchPush({ node: 7, chars: 0 }, { node: 8, chars: 500 }), null);
});
