// 字数口径的显示逻辑：挑哪个数、点一下换成哪个、字典键对不对得上。
//
// 这里**不测"数怎么算"**——那是核心的事（`text::WordCaliber` 有单测）。这里只测界面这一层：
// 认不认识核心给的码、循环顺序与核心一致、字典键拿得到文案（写错键就会在界面上露出来）。

import { test } from "node:test";
import assert from "node:assert/strict";

import { t } from "../locales/index.ts";
import {
  CALIBERS,
  asCaliber,
  asLanguage,
  caliberLabelKey,
  countUnitKey,
  languageLabelKey,
  nextCaliber,
  nextLanguage,
  pickCount,
} from "./wordcount.ts";

const COUNTS = { char_count: 12, chars_no_punct: 9, word_count: 7 };

test("口径循环顺序与核心一致：逐字 → 不含标点 → 按词 → 回到逐字", () => {
  assert.deepEqual([...CALIBERS], ["chars", "chars_no_punct", "words"]);
  assert.equal(nextCaliber("chars"), "chars_no_punct");
  assert.equal(nextCaliber("chars_no_punct"), "words");
  assert.equal(nextCaliber("words"), "chars", "转一圈回到第一档");
});

test("认不出来的口径退回「逐字」，不让界面没数可显示", () => {
  assert.equal(asCaliber(null), "chars");
  assert.equal(asCaliber("nonsense"), "chars");
  assert.equal(nextCaliber("nonsense"), "chars_no_punct", "从第一档往后走");
});

test("语言循环：中 → 英 → 日 → 中", () => {
  assert.equal(nextLanguage("zh"), "en");
  assert.equal(nextLanguage("en"), "ja");
  assert.equal(nextLanguage("ja"), "zh");
  assert.equal(asLanguage("fr"), "zh", "认不出来按中文");
});

test("pickCount 取当前口径那一个数", () => {
  assert.equal(pickCount(COUNTS, "chars"), 12);
  assert.equal(pickCount(COUNTS, "chars_no_punct"), 9);
  assert.equal(pickCount(COUNTS, "words"), 7);
  assert.equal(pickCount(COUNTS, null), 12, "认不出来按逐字");
});

test("字典键都查得到文案（写错键会在界面上露出来）", () => {
  for (const caliber of CALIBERS) {
    const label = caliberLabelKey(caliber);
    assert.notEqual(t(label), label, `口径名缺文案：${label}`);
    const unit = countUnitKey(caliber);
    assert.notEqual(t(unit, { count: 1 }), unit, `计数单位缺文案：${unit}`);
  }
  for (const language of ["zh", "en", "ja"]) {
    const label = languageLabelKey(language);
    assert.notEqual(t(label), label, `语言名缺文案：${label}`);
  }
  // 按词该报「词」、逐字该报「字」——两个口径的读数单位不能混
  assert.match(t(countUnitKey("words"), { count: 3 }), /词/);
  assert.match(t(countUnitKey("chars"), { count: 3 }), /字/);
});
