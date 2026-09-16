// 故事总纲的纯逻辑验收：**摘要取哪一句、算不算没写**。
//
// 这一层没有 DOM：组件只管把它们摆上去。

import { test } from "node:test";
import assert from "node:assert/strict";

import { storylineEmpty, storylineExcerpt } from "./storyline.ts";

test("没写：全是空白（空行、空格、换行）都算没写", () => {
  assert.equal(storylineEmpty(""), true);
  assert.equal(storylineEmpty("   \n\n \t "), true);
  assert.equal(storylineEmpty("题材：悬疑"), false);
});

test("摘要取第一句有内容的话（空行跳过、行首空格修剪）", () => {
  assert.equal(storylineExcerpt("\n\n  主线：等他回来。\n卖点：灯下是谁。"), "主线：等他回来。");
  assert.equal(storylineExcerpt(""), "还没写——点这里写整本书讲什么");
  assert.equal(storylineExcerpt("  \n \n"), "还没写——点这里写整本书讲什么");
});

test("摘要太长就截断（表头那一行是「知道写没写」，不是阅读区）", () => {
  const long = "主线：" + "很长".repeat(50);
  const excerpt = storylineExcerpt(long, 10);
  assert.equal(excerpt.length, 11, "10 个字 + 省略号");
  assert.ok(excerpt.endsWith("…"));
  assert.equal(storylineExcerpt("短话", 10), "短话", "没超长就原样给");
});
