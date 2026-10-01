// 拆章的纯逻辑验收：**光标落在哪儿 = 从第几段之前切开**。
//
// 这一条的口径是作者拍板的：**按光标所在那一段的开头切**（段落完整性是结构上保证的，
// 不存在"按字数硬切把一段拦腰截断"）。所以这里盯的是四件事：
// ① 段落 → 下标的换算是逐段数的、不是按字数估的；
// ② 段内任意位置与段首给同一个答案（可预期）；
// ③ 切在开头 / 末尾 / 只有一段 —— 一律 `null`（界面据此把按钮置灰）；
// ④ **空段落不算一段**（与库里那份正文同一口径：空行只当分隔符）。

import { test } from "node:test";
import assert from "node:assert/strict";

import { canSplit, splitPointOf, type DocLike, type DocNode } from "./split.ts";

/** 造一个假文档：每个字符串一段（`""` = 空段落）。段落在文档里占 `文字长度 + 2` 个位置。 */
function doc(texts: string[]): DocLike {
  const nodes: { node: DocNode; offset: number }[] = [];
  let offset = 0;
  for (const text of texts) {
    nodes.push({
      node: { type: { name: "paragraph" }, nodeSize: text.length + 2, textContent: text },
      offset,
    });
    offset += text.length + 2;
  }
  return {
    forEach: (visit) => {
      for (const { node, offset } of nodes) visit(node, offset);
    },
  };
}

/** 第 i 段的段首位置（文档里的位置，第一段从 1 起）。 */
function startOf(texts: string[], index: number): number {
  let offset = 1;
  for (let i = 0; i < index; i += 1) offset += texts[i].length + 2;
  return offset;
}

test("光标落在某一段里 ⇒ 从那一段之前切", () => {
  const texts = ["第一段。", "第二段。", "第三段。"];
  const target = doc(texts);
  // 光标丢在第二段正中间
  const point = splitPointOf(target, startOf(texts, 1) + 2);
  assert.deepEqual(point, { at: 1, total: 3, kept: 1, moved: 2 });
});

test("段首、段中、段尾给的都是同一个答案（除段尾进下一段之前）", () => {
  const texts = ["一。", "二。", "三。"];
  const target = doc(texts);
  const start = startOf(texts, 1);
  // 段首与段中：都在第二段之前切
  assert.equal(splitPointOf(target, start)?.at, 1);
  assert.equal(splitPointOf(target, start + 1)?.at, 1);
  // 正好落在第二段末尾：那一段算进前半（切点在第三段之前）
  assert.equal(splitPointOf(target, start + texts[1].length + 1)?.at, 2);
});

test("切在开头 / 末尾 / 只有一段：都拆不了", () => {
  const texts = ["一。", "二。"];
  const target = doc(texts);
  assert.equal(splitPointOf(target, startOf(texts, 0)), null, "切在第一段之前 ⇒ 前半是空的");
  const end = startOf(texts, 1) + texts[1].length + 1;
  assert.equal(splitPointOf(target, end), null, "切在章末 ⇒ 后半是空的");
  assert.equal(splitPointOf(doc(["只有一段。"]), 2), null, "只有一段：没得拆");
  assert.equal(splitPointOf(doc([]), 1), null, "空文档：没得拆");
});

test("空段落不算一段（与库里那份正文同一口径）", () => {
  const texts = ["一。", "", "二。", "三。"];
  const target = doc(texts);
  // 有 3 个**非空**段；光标落在「二。」上 ⇒ 非空段的下标 1
  const point = splitPointOf(target, startOf(texts, 2) + 1);
  assert.deepEqual(point, { at: 1, total: 3, kept: 1, moved: 2 });
  // 光标落在那个空段落里 ⇒ 也按"下一段之前"算（空行只是分隔符）
  assert.equal(splitPointOf(target, startOf(texts, 1) + 1)?.at, 1);
});

test("非段落节点不参与数段（代码块之类不算一段正文）", () => {
  const target: DocLike = {
    forEach: (visit) => {
      visit({ type: { name: "paragraph" }, nodeSize: 5, textContent: "一段。" }, 0);
      visit({ type: { name: "codeBlock" }, nodeSize: 20, textContent: "code" }, 5);
      visit({ type: { name: "paragraph" }, nodeSize: 5, textContent: "二段。" }, 25);
    },
  };
  assert.deepEqual(splitPointOf(target, 26), { at: 1, total: 2, kept: 1, moved: 1 });
});

test("canSplit：没打开编辑器 / 切不了 —— 都返回 false", () => {
  const texts = ["一。", "二。"];
  assert.equal(canSplit(null), false, "没打开编辑器");
  const atSecond = { state: { doc: doc(texts), selection: { from: startOf(texts, 1) } } };
  assert.equal(canSplit(atSecond), true);
  const atFirst = { state: { doc: doc(texts), selection: { from: startOf(texts, 0) } } };
  assert.equal(canSplit(atFirst), false, "光标在第一段：切不了");
});
