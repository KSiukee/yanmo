// 正文序列化的测试：段落进出往返、转义、空文本。
//
// 库里存的是纯文本（段落之间空行），所以"进编辑器"和"回库里"这两条路必须严格对称——
// 差一个空行，作者的排版就会在下次打开时变化。

import { test } from "node:test";
import assert from "node:assert/strict";

import { paragraphsOf, textToHtml } from "./doc.ts";

test("空文本没有段落", () => {
  assert.deepEqual(paragraphsOf(""), []);
  assert.deepEqual(paragraphsOf("\n\n\n"), []);
  assert.equal(textToHtml(""), "");
});

test("空行分段，段内换行折进同一段", () => {
  assert.deepEqual(paragraphsOf("第一段\n\n第二段\n\n第三段"), ["第一段", "第二段", "第三段"]);
  assert.deepEqual(paragraphsOf("第一段\n仍然第一段\n\n第二段"), ["第一段\n仍然第一段", "第二段"]);
});

test("多出来的空白与空段被收敛", () => {
  assert.deepEqual(paragraphsOf("  第一段  \n\n\n\n  第二段  "), ["第一段", "第二段"]);
});

test("转义尖括号与和号，避免正文被当成标签", () => {
  assert.equal(textToHtml("1 < 2 & 3 > 2"), "<p>1 &lt; 2 &amp; 3 &gt; 2</p>");
});

test("多段转成多个 p", () => {
  assert.equal(textToHtml("甲\n\n乙"), "<p>甲</p><p>乙</p>");
});
