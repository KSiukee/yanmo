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

// 字典是 UTF-8 文本文件，一拍脑袋存成别的编码就会整片变乱码——这类痕迹要机械拦住。
test("字典里不许有乱码痕迹（U+FFFD）与不可见控制字符", () => {
  const replacement: string[] = [];
  const control: string[] = [];
  for (const key of keys()) {
    const text = t(key);
    if (text.includes("\uFFFD")) replacement.push(key);
    // 允许的正常可见字符之外：控制字符与双向排版控制符一律不许进字典
    // （RTL 覆盖符能把一整行文字显示顺序调乱，那是"看起来像乱码"的典型来源）
    if (/[\u0000-\u0008\u000B\u000C\u000E-\u001F\u202A-\u202E\u2066-\u2069]/.test(text)) {
      control.push(key);
    }
  }
  assert.deepEqual(replacement, [], `这些键的文案里有乱码替换字符：${replacement.join("、")}`);
  assert.deepEqual(control, [], `这些键的文案里有控制字符：${control.join("、")}`);
});

test("字典值不许带首尾空白（写歪了看不出来，但会挤坏排版）", () => {
  const bad = keys().filter((key) => t(key) !== t(key).trim());
  assert.deepEqual(bad, [], `这些键的文案有首尾空白：${bad.join("、")}`);
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
