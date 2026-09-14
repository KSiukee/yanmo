// 正文编辑器扩展清单的验收：**只允许段落与文字**。
//
// 这里的断言不是核对那份配置对象，而是**问真正的 schema**——配置键写错、StarterKit 换了名字、
// 将来它新增一个标记，都会在这里露出来。核对自己写的那份配置等于自证，挡不住这些。

import { test } from "node:test";
import assert from "node:assert/strict";

import { getSchema } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";

import { PLAIN_TEXT_KIT, plainTextExtensions } from "./extensions.ts";

/** 允许留在正文里的节点：多一个都算"隐形结构"。 */
const ALLOWED_NODES = ["doc", "paragraph", "text"];

test("落盘的正文只可能是段落与文字：一个标记都不许有", () => {
  const schema = getSchema(plainTextExtensions());
  assert.deepEqual(
    Object.keys(schema.marks),
    [],
    "正文里出现了标记——它们落盘后一个字都不剩，正是「改了但看不出」的成因",
  );
  assert.deepEqual(
    Object.keys(schema.nodes).sort(),
    ALLOWED_NODES,
    "正文里出现了段落之外的节点（列表 / 标题 / 引用 / 分割线 / 段内换行都算）",
  );
});

test("对照完整版 StarterKit：它的标记与块级节点一个都没漏关", () => {
  const full = getSchema([StarterKit]);
  const plain = getSchema(plainTextExtensions());
  // 先确认"对照物"本身不是空的，否则这条测试就是空转
  assert.ok(
    Object.keys(full.marks).length > 0,
    "完整版 StarterKit 里一个标记都没有？那这条对照失去意义，先看一眼依赖版本",
  );
  const leaked = Object.keys(full.marks).filter((mark) => mark in plain.marks);
  assert.deepEqual(leaked, [], `还有没关掉的标记：${leaked.join("、")}`);
  const keptNodes = Object.keys(full.nodes).filter((node) => node in plain.nodes && !ALLOWED_NODES.includes(node));
  assert.deepEqual(keptNodes, [], `还有没关掉的块级节点：${keptNodes.join("、")}`);
});

test("关格式不能顺手把撤销也关掉", () => {
  const options = PLAIN_TEXT_KIT as Record<string, unknown>;
  assert.notEqual(options.undoRedo, false, "撤销重做必须留着——写作软件没有撤销是灾难");
  assert.notEqual(options.trailingNode, false, "结尾补空段也留着，不然最后一段难敲");
});
