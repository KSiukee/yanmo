// 分类归属（体裁 / 标签）这一栏纯规则的验收：**一行一个标签、去重、保序**，
// 以及「按主题」到底按什么判据摆——这一档的全部复杂度都在这儿（不碰界面与核心）。

import { test } from "node:test";
import assert from "node:assert/strict";

import type { ArticleEntry } from "../api/collection";
import { GENRE_PRESETS, genreOf, joinTags, parseTags, sortByTheme, themeOf } from "./classify.ts";

function entry(partial: Partial<ArticleEntry> = {}): ArticleEntry {
  return {
    id: 1,
    title: "一篇",
    status: "draft",
    word_count: 10,
    char_count: 20,
    chars_no_punct: 18,
    created_at: 0,
    updated_at: 0,
    genre: "",
    tags: "",
    ...partial,
  };
}

test("标签按行读：去首尾空白、丢掉空行、去掉重复，**保留作者写的先后**", () => {
  assert.deepEqual(parseTags("  悬疑 \n\n都市\r\n悬疑\n"), ["悬疑", "都市"]);
});

test("逗号与空格**不是**分隔符：一行才是一个标签", () => {
  assert.deepEqual(parseTags("科幻, 太空歌剧"), ["科幻, 太空歌剧"]);
  assert.deepEqual(parseTags("硬 科幻"), ["硬 科幻"]);
});

test("标签写回一栏文本是「一行一个」，与读是一对", () => {
  const tags = parseTags("甲\n乙\n甲");
  assert.equal(joinTags(tags), "甲\n乙");
  assert.deepEqual(parseTags(joinTags(tags)), tags);
});

test("主题＝作者写的**第一个**标签；一个都没写就是空串（不替他想一个）", () => {
  assert.equal(themeOf(entry({ tags: "太空\n群像" })), "太空");
  assert.equal(themeOf(entry({ tags: "" })), "");
  assert.equal(themeOf(entry({ tags: "  \n " })), "");
});

test("体裁去首尾空白，内部写法一个字不改", () => {
  assert.equal(genreOf(entry({ genre: "  社会 科幻 " })), "社会 科幻");
});

test("按主题：先按体裁聚、组内按主题，都没填的排最后", () => {
  const ids = (list: ArticleEntry[]) => sortByTheme(list).map((item) => item.id);
  const scifi = entry({ id: 1, genre: "科幻", tags: "太空" });
  const mystery = entry({ id: 2, genre: "悬疑", tags: "密室" });
  const blank = entry({ id: 3, genre: "", tags: "" });
  // 组与组之间的先后按体裁名**字面量序**（「悬疑」在「科幻」前）：这里只要"同类的挨在一起、
  // 没填的垫底"——软件不替作者规定哪个体裁更靠前。
  assert.deepEqual(ids([mystery, blank, scifi]), [2, 1, 3]);
});

test("并列时保持现在这个先后（排序是稳定的，手拖过的顺序不被无谓打乱）", () => {
  const first = entry({ id: 1, genre: "科幻", tags: "太空" });
  const second = entry({ id: 2, genre: "科幻", tags: "太空" });
  assert.deepEqual(sortByTheme([first, second]).map((item) => item.id), [1, 2]);
  assert.deepEqual(sortByTheme([second, first]).map((item) => item.id), [2, 1]);
});

test("一个分类都没填：顺序原样（这一档不假装懂主题）", () => {
  const list = [entry({ id: 7 }), entry({ id: 3 }), entry({ id: 5 })];
  assert.deepEqual(sortByTheme(list).map((item) => item.id), [7, 3, 5]);
});

test("常用体裁是**候选**：一列非空的字典键，不许写成死汉字", () => {
  assert.ok(GENRE_PRESETS.length >= 8, "常用档太少就起不到「不必手打」的作用");
  for (const key of GENRE_PRESETS) {
    assert.match(key, /^shelf\.genre_preset_/, "每个候选都得是字典键（核心与界面都不许写死文案）");
  }
  assert.equal(new Set(GENRE_PRESETS).size, GENRE_PRESETS.length, "候选里不许有重复");
});
