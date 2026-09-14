// 标题拆分/合并的验收：**宏不露给作者，也不会在改名时被弄丢**。
//
// 这是真机上暴露出来的：改名框里直接摊出 `{$N}章 复活节雕塑`，作者一删就把编号删了。

import { test } from "node:test";
import assert from "node:assert/strict";

import { composeTitle, renderedPrefix, splitTitle } from "./title-edit.ts";

test("拆：宏那一小节算骨架，名字留给作者编辑", () => {
  const parts = splitTitle("第{$N}章 复活节雕塑");
  assert.equal(parts.prefix, "第{$N}章 ");
  assert.equal(parts.name, "复活节雕塑");
});

test("拆：只有骨架、还没起名时，名字是空的", () => {
  const parts = splitTitle("第{$N}章");
  assert.equal(parts.prefix, "第{$N}章");
  assert.equal(parts.name, "");
});

test("拆：真机那一串（`{$N}章 复活节雕塑`）也认得出来", () => {
  const parts = splitTitle("{$N}章 复活节雕塑");
  assert.equal(parts.prefix, "{$N}章 ");
  assert.equal(parts.name, "复活节雕塑", "别把宏丢给作者看");
});

test("拆：别的档位与写法也认（中文数字 / 补零 / 卷）", () => {
  assert.deepEqual(splitTitle("第{$N_ZH}章 灯"), { prefix: "第{$N_ZH}章 ", name: "灯" });
  assert.deepEqual(splitTitle("第{$N:3}章 门"), { prefix: "第{$N:3}章 ", name: "门" });
  assert.deepEqual(splitTitle("第{$N}卷 夜行"), { prefix: "第{$N}卷 ", name: "夜行" });
});

test("拆：本来就没有宏的标题（自起名不占号），整串都归作者", () => {
  assert.deepEqual(splitTitle("序章"), { prefix: "", name: "序章" });
  assert.deepEqual(splitTitle("番外 一"), { prefix: "", name: "番外 一" });
  assert.deepEqual(splitTitle(""), { prefix: "", name: "" });
});

test("拆：宏写在中间时，骨架一直算到宏那一小节（宁少编辑，不丢宏）", () => {
  const parts = splitTitle("灯 第{$N}章 门");
  assert.equal(parts.prefix, "灯 第{$N}章 ");
  assert.equal(parts.name, "门");
});

test("合：骨架原样 + 一个空格 + 名字（这就是显示层要的 `第1章 章名`）", () => {
  const parts = splitTitle("第{$N}章 复活节雕塑");
  assert.equal(composeTitle(parts, "复活节灯"), "第{$N}章 复活节灯");
  assert.equal(composeTitle(parts, "  元宵  "), "第{$N}章 元宵", "两头空白不留");
});

test("合：名字清空只剩骨架——编号还在，只是这一章没名字了", () => {
  const parts = splitTitle("第{$N}章 复活节雕塑");
  assert.equal(composeTitle(parts, ""), "第{$N}章");
  assert.equal(composeTitle({ prefix: "", name: "序章" }, "  "), "", "自起名清空就是空标题");
});

test("拆了再合：原样回得去（改名前后的往返不许漂）", () => {
  for (const raw of ["第{$N}章 复活节雕塑", "{$N}章 复活节雕塑", "第{$N}章", "序章"]) {
    const parts = splitTitle(raw);
    assert.equal(composeTitle(parts, parts.name), raw, `往返漂了：${raw}`);
  }
});

test("输入框前面那行灰字：骨架渲染后的样子（`第1章`）", () => {
  const parts = splitTitle("第{$N}章 复活节雕塑");
  assert.equal(renderedPrefix("第1章 复活节雕塑", parts), "第1章");
  // 还只有骨架、没名字时：整串就是骨架
  const bare = splitTitle("第{$N}章");
  assert.equal(renderedPrefix("第3章", bare), "第3章");
  // 自起名的标题前面不该冒出任何灰字
  assert.equal(renderedPrefix("序章", splitTitle("序章")), "");
});
