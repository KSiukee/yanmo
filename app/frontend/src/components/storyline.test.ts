// 故事总纲的纯逻辑验收：**摘要取哪一句、算不算没写**。
//
// 这一层没有 DOM：组件只管把它们摆上去。

import { test } from "node:test";
import assert from "node:assert/strict";

import { storylineEmpty } from "./storyline.ts";

test("没写：全是空白（空行、空格、换行）都算没写", () => {
  assert.equal(storylineEmpty(""), true);
  assert.equal(storylineEmpty("   \n\n \t "), true);
  assert.equal(storylineEmpty("题材：悬疑"), false);
});
