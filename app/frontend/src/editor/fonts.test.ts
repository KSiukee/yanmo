// 正文字体的取值与字体栈：**认不出的一律回默认**，自定义字体也要拼得安全。

import { test } from "node:test";
import assert from "node:assert/strict";

import { asEditorFont, fontFamilyName, fontFileName, fontStackOf } from "./fonts.ts";

test("认不出的字体值回默认（宋体那档）", () => {
  assert.equal(asEditorFont({ editor_font: "comic-sans" }), "serif");
  assert.equal(asEditorFont({ editor_font: "" }), "serif");
  assert.equal(asEditorFont(null), "serif");
  assert.equal(asEditorFont({}), "serif");
});

test("五档预设原样认，`family:<名字>` 也认", () => {
  for (const code of ["serif", "sans", "kai", "yahei", "dengxian"]) {
    assert.equal(asEditorFont({ editor_font: code }), code);
  }
  assert.equal(asEditorFont({ editor_font: "family:思源宋体 CN" }), "family:思源宋体 CN");
  assert.equal(fontFamilyName("family:思源宋体 CN"), "思源宋体 CN");
  assert.equal(fontFamilyName("kai"), null);
});

test("字体栈：预设各不相同；自定义名字进栈并去掉引号", () => {
  const serif = fontStackOf("serif");
  const kai = fontStackOf("kai");
  assert.notEqual(serif, kai, "两档预设不该是同一份字体栈");
  assert.ok(serif.includes("serif"));
  assert.ok(kai.includes("KaiTi"));

  const custom = fontStackOf('family:我的"字体\\');
  assert.ok(custom.startsWith('"我的字体"'), custom);
  assert.ok(custom.endsWith("serif"));
});

test("字体文件：file: 原样认，拼栈时用统一的族名", () => {
  assert.equal(asEditorFont({ editor_font: "file:font-abc.ttf" }), "file:font-abc.ttf");
  assert.equal(fontFileName("file:font-abc.ttf"), "font-abc.ttf");
  assert.equal(fontFileName("kai"), null);
  const stack = fontStackOf("file:font-abc.ttf");
  assert.ok(stack.startsWith('"YanmoCustomFont"'), stack);
  assert.ok(stack.endsWith("serif"));
});
