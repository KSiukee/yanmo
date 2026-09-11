// 焦点策略的测试：**历史章不聚焦（防误触）、该写的章接着写**。
//
// 表驱动：策略是纯函数，把各种组合一次钉住，比只测两个例子更能压住分支。

import { test } from "node:test";
import assert from "node:assert/strict";

import { focusPlan, type FocusInput } from "./focus.ts";

function input(overrides: Partial<FocusInput> = {}): FocusInput {
  return { fresh: false, had_cursor: false, is_latest: false, jump_to_end: true, ...overrides };
}

test("打开历史章：一律把光标挪出正文（看，不写）", () => {
  // 冷门组合也一起钉住：读过没读过、开关关没关，历史章都不该聚焦
  for (const had_cursor of [false, true]) {
    for (const jump_to_end of [false, true]) {
      assert.equal(
        focusPlan(input({ is_latest: false, had_cursor, jump_to_end })),
        "blur",
        `历史章必须 blur（had_cursor=${had_cursor}, jump=${jump_to_end}）`,
      );
    }
  }
});

test("首次打开最新章：跳到段末并聚焦（开关开着）", () => {
  assert.equal(focusPlan(input({ is_latest: true })), "focus-end");
});

test("开关关掉：最新章也只是打开，不聚焦", () => {
  assert.equal(focusPlan(input({ is_latest: true, jump_to_end: false })), "leave");
});

test("读过的最新章：回原位，绝不拽到段尾", () => {
  assert.equal(focusPlan(input({ is_latest: true, had_cursor: true })), "leave");
});

test("刚新建/补写的章：不管在第几章、有没有读过，都接着写", () => {
  assert.equal(focusPlan(input({ fresh: true })), "focus-end", "中间插的空章也要能直接写");
  assert.equal(focusPlan(input({ fresh: true, is_latest: true })), "focus-end");
  assert.equal(focusPlan(input({ fresh: true, had_cursor: true })), "focus-end", "刚建的不可能读过，但规则要一致");
  assert.equal(focusPlan(input({ fresh: true, jump_to_end: false })), "leave", "关掉开关就连新建也不抢焦点");
});
