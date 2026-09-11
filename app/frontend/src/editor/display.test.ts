// 展示口径的测试：**数字与时间怎么念**，只此一处说了算。

import { test } from "node:test";
import assert from "node:assert/strict";

import { formatWhen, formatWords } from "./display.ts";
import { nextWorkAfterDelete, shelfKindLabel, shelfLabel } from "./shelf.ts";
import type { ShelfEntry } from "../api/core.ts";

const DAY = 86_400_000;

function entry(id: number, chapters: number, words: number, opened: number | null = null): ShelfEntry {
  return {
    id,
    kind: "novel",
    title: `第${id}本`,
    chapters,
    word_count: words,
    opened_at: opened,
    created_at: 0,
    updated_at: 0,
  };
}

test("字数给人看：一万以上换成「万」", () => {
  assert.equal(formatWords(0), "0");
  assert.equal(formatWords(9999), "9999");
  assert.equal(formatWords(10000), "1万");
  assert.equal(formatWords(12345), "1.2万");
  assert.equal(formatWords(1234567), "123.5万");
});

test("最近打开时间给人看：今天 / 昨天 / N 天前 / 日期 / 从没打开", () => {
  const now = 1_700_000_000_000;
  assert.equal(formatWhen(null, now), "还没打开过");
  assert.equal(formatWhen(now - 1000, now), "今天");
  assert.equal(formatWhen(now - DAY, now), "昨天");
  assert.equal(formatWhen(now - 5 * DAY, now), "5 天前");
  assert.match(formatWhen(now - 100 * DAY, now), /^\d{4}-\d{2}-\d{2}$/, "太久远就报日期");
});

test("书架上那一行的小字：有章报章数，单篇只报字数", () => {
  assert.equal(shelfLabel({ chapters: 12, word_count: 34000 }), "12 章 · 3.4万 字");
  assert.equal(shelfLabel({ chapters: 0, word_count: 800 }), "800 字");
});

test("作品类型的显示名", () => {
  assert.equal(shelfKindLabel("novel"), "长篇");
  assert.equal(shelfKindLabel("collection"), "短篇集");
  assert.equal(shelfKindLabel("article"), "单篇");
  assert.equal(shelfKindLabel("whatever"), "单篇", "不认识的取值当单篇，不崩");
});

test("删掉当前这本之后：开列表里的下一本；一本不剩交给核心", () => {
  const shelf = [entry(3, 1, 10), entry(2, 1, 10), entry(1, 1, 10)];
  assert.equal(nextWorkAfterDelete(shelf, 3), 2, "删的是第一本，就开它后面那本");
  assert.equal(nextWorkAfterDelete(shelf, 2), 3, "删的是中间那本，就开排最前的那本");
  assert.equal(nextWorkAfterDelete([entry(9, 1, 10)], 9), null, "一本不剩 → 交给核心建默认的");
});
