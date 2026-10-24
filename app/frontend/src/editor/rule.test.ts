// 稿纸横线的验收：**默认档、认不出的码、以及"一格多高"那条换算**——
// 三件都是"看一眼就知道对不对"的事，但它们一旦错了，作者看到的是"线从字底下漂开"
// 或者"选了没反应"，比报错更难查。

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  asRuleStyle,
  DEFAULT_RULE_STYLE,
  RULE_STYLES,
  ruleStep,
  ruleStyle,
} from "./rule.ts";
import { asTypography, TYPOGRAPHY_DEFAULT } from "./typography.ts";

test("没设过 → 不显示；认不出的码也当没设过", () => {
  assert.equal(asRuleStyle(null), "none");
  assert.equal(asRuleStyle({}), "none");
  assert.equal(asRuleStyle({ editor_rule_style: "dotted" }), "none");
  assert.equal(asRuleStyle({ editor_rule_style: "" }), "none");
  assert.equal(DEFAULT_RULE_STYLE, "none");
});

test("设过就照它来（两档都认得）", () => {
  assert.equal(asRuleStyle({ editor_rule_style: "solid" }), "solid");
  assert.equal(asRuleStyle({ editor_rule_style: "dashed" }), "dashed");
  assert.equal(asRuleStyle({ editor_rule_style: "none" }), "none");
});

test("三档穷举：不重不漏，默认那一档在表里", () => {
  assert.deepEqual([...RULE_STYLES], ["solid", "dashed", "none"]);
  assert.equal(new Set(RULE_STYLES).size, RULE_STYLES.length);
  assert.ok(RULE_STYLES.includes(DEFAULT_RULE_STYLE));
});

test("一格横线的高度 = 字号 × 行距（线才落在字底下）", () => {
  // 默认：17px × 1.9 = 32.3px
  assert.equal(ruleStep(null), 32.3);
  assert.deepEqual(ruleStyle(null), { "--ym-rule-step": "32.3px" });
  // 设过就跟着排版那三个值走
  assert.equal(ruleStep({ editor_font_size: 22, editor_line_height: 220 }), 48.4);
  assert.equal(ruleStep({ editor_font_size: 12, editor_line_height: 110 }), 13.2);
});

test("行距换一档线就跟着换一档（两处算的必须是同一个数）", () => {
  // 这条钉的是"排版"与"稿纸"两栏不许各算一份：改行距必须同时改线距
  for (const value of [110, 190, 200, 260]) {
    const values = { editor_font_size: 17, editor_line_height: value };
    const { size, line } = asTypography(values);
    assert.equal(ruleStep(values), Math.round(size * line) / 100);
  }
});

test("坏值 / 越界一律跟排版那份默认走（不把线距算成 NaN 或负数）", () => {
  assert.equal(ruleStep({ editor_font_size: 0, editor_line_height: 0 }), 32.3);
  assert.equal(ruleStep({ editor_font_size: Number.NaN }), 32.3);
  assert.equal(ruleStep({ editor_font_size: 1000, editor_line_height: 9999 }), 78);
  // 兜底档与排版自己的默认同源（改一处忘另一处，这条会红）
  // 线型不同、那一格高度不变（档位靠属性走，不靠变量——两档共用同一份格距）
  assert.deepEqual(ruleStyle({ editor_rule_style: "solid" }), ruleStyle(null));
  assert.equal(
    ruleStep({}),
    Math.round(TYPOGRAPHY_DEFAULT.size * TYPOGRAPHY_DEFAULT.line) / 100,
  );
});

test("小数位不丢（四舍五入回整数会让线一行一行地漂开）", () => {
  // 17 × 195 = 33.15：不许变成 33
  assert.equal(ruleStep({ editor_font_size: 17, editor_line_height: 195 }), 33.15);
});
