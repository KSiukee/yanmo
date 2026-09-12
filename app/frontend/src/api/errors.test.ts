// 「核心为什么失败」到「界面上那句话」这条链路的测试。
//
// 它守着一条纪律：**界面不拼句子**——句子只在 `src/locales/zh-Hans.json` 里，
// 这里换了个码，界面上就该换个说法。

import { test } from "node:test";
import assert from "node:assert/strict";

import { t } from "../locales/index.ts";
import {
  asError,
  CoreError,
  CoreUnavailableError,
  dropLoneSurrogates,
  healText,
} from "./errors.ts";

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
  // 原始文本留着（排查要用），但句子的主体是中文——**别把英文解析错误直接摆给作者**
  assert.match(e.message, /^这一步没能完成（数据读不出来）：/);
  assert.ok(e.message.includes("ipc 断了"));
});

test("半个字符（落单代理项）要能修掉：它是 Rust 那边拒收整段文字的唯一原因", () => {
  // 正常的一对代理项（真 emoji）不能动
  assert.equal(dropLoneSurrogates("😀"), "😀");
  assert.equal(dropLoneSurrogates("汉字"), "汉字");
  // 落单的高位/低位都要去掉
  assert.equal(dropLoneSurrogates("\uD800"), "");
  assert.equal(dropLoneSurrogates("\uDC00"), "");
  assert.equal(dropLoneSurrogates("第\uD83D章"), "第章", "被切一半的 emoji 直接丢掉");
  assert.equal(dropLoneSurrogates("😀\uD83D😀"), "😀😀", "好的留着，坏的丢掉");
});

test("healText 递归修参数：字符串、数组、朴素对象都走一遍", () => {
  assert.deepEqual(healText({ title: "第\uD83D章", n: 3, ok: true, none: null }), {
    title: "第章",
    n: 3,
    ok: true,
    none: null,
  });
  assert.deepEqual(healText({ arr: ["a\uD800", "b"] }), { arr: ["a", "b"] });
  assert.deepEqual(healText({ deep: { inner: "x\uDC00" } }), { deep: { inner: "x" } });
  const date = new Date(0);
  assert.equal(healText(date), date, "不是朴素对象就原样返回，别克隆坏了");
});

test("既不是字符串也没有码：也按核心不可达处理", () => {
  assert.ok(asError({ detail: "没有码" }) instanceof CoreUnavailableError);
  assert.ok(asError(null) instanceof CoreUnavailableError);
  assert.ok(asError(undefined) instanceof CoreUnavailableError);
});
