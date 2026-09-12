// 「核心为什么失败」到「界面上那句话」这条链路的测试。
//
// 它守着一条纪律：**界面不拼句子**——句子只在 `src/locales/zh-Hans.json` 里，
// 这里换了个码，界面上就该换个说法。

import { test } from "node:test";
import assert from "node:assert/strict";

import { t } from "../locales/index.ts";
import { asError, CoreError, CoreUnavailableError } from "./errors.ts";

test("带码的失败按字典渲染成句子，并保留码与参数", () => {
  const e = asError({
    code: "work.not_trashed",
    params: { work_id: "7" },
    detail: "回收站里没有这本书：7",
  });
  assert.ok(e instanceof CoreError, "有码就该是 CoreError");
  assert.equal(e.message, "回收站里没有这本书：7");
  assert.equal((e as CoreError).code, "work.not_trashed");
  assert.equal((e as CoreError).params.work_id, "7");
});

test("没有参数也能渲染（不带占位符的那些码）", () => {
  const e = asError({ code: "work.title_empty", params: {} });
  assert.equal(e.message, "作品标题不能为空");
});

test("码没见过时：句子退化成码本身，但仍然是 CoreError（不假装是别的毛病）", () => {
  const e = asError({ code: "not.in.the.table", params: {} });
  assert.ok(e instanceof CoreError);
  assert.equal(e.message, "error.not.in.the.table");
  assert.equal((e as CoreError).code, "not.in.the.table");
});

test("参数取值原样填进模板，不从别处取字", () => {
  assert.equal(t("error.node.gone", { node_id: 42 }), "节点不存在或已删除：42");
});

test("字符串失败 = 核心不可达（那是程序出了问题，不是这件事做不成）", () => {
  const e = asError("ipc 断了");
  assert.ok(e instanceof CoreUnavailableError);
  assert.equal(e.message, "ipc 断了");
});

test("既不是字符串也没有码：也按核心不可达处理", () => {
  assert.ok(asError({ detail: "没有码" }) instanceof CoreUnavailableError);
  assert.ok(asError(null) instanceof CoreUnavailableError);
  assert.ok(asError(undefined) instanceof CoreUnavailableError);
});
