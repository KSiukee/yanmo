// 界面字典自己的验收：**文案集中在一处之后，这一处也得有人看着**。
//
// 与 Rust 侧 `app/src/error.rs` 的穷举测试分工：
// - 那边管「错误码 ↔ 字典」对得上（漏一个码 = 界面上是「未知错误」）；
// - 这边管字典本身别写坏（空值、坏占位符），以及 `t()` 的兜底行为。

import { test } from "node:test";
import assert from "node:assert/strict";

import { has, keys, t } from "./index.ts";

test("字典里没有空值——空字符串是最难查的一类界面故障", () => {
  const empty = keys().filter((key) => t(key).trim() === "");
  assert.deepEqual(empty, [], `这些键的文案是空的：${empty.join("、")}`);
});

test("占位符名字规矩：只用小写字母与下划线（将来换 i18n 库也好认）", () => {
  const bad: string[] = [];
  for (const key of keys()) {
    for (const match of t(key).matchAll(/\{([^}]*)\}/g)) {
      if (!/^[a-z_]+$/.test(match[1])) bad.push(`${key} → {${match[1]}}`);
    }
  }
  assert.deepEqual(bad, [], `这些占位符名字不合规矩：${bad.join("、")}`);
});

test("t 填参数；参数缺了就原样留着，不静默吞掉", () => {
  assert.equal(t("display.days_ago", { days: 3 }), "3 天前");
  assert.equal(t("display.days_ago", { other: 1 }), "{days} 天前");
});

test("查不到的键原样返回键名（一眼看得见，而不是空白）", () => {
  assert.equal(t("nothing.here"), "nothing.here");
  assert.equal(has("nothing.here"), false);
  assert.equal(has("display.today"), true);
});
