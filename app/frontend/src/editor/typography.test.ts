// 排版档位的验收：**默认值、越界夹住、CSS 变量换算**——三件都是"看一眼就知道对不对"的事，
// 但它们一旦错了，作者看到的是"字大得离谱"或者"调了没反应"，比报错更难查。

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  asTypography,
  TYPOGRAPHY_DEFAULT,
  TYPOGRAPHY_KNOBS,
  typographyStyle,
} from "./typography.ts";

test("没设过 → 默认档（17px / 1.9 / 不加字距）", () => {
  assert.deepEqual(asTypography(null), TYPOGRAPHY_DEFAULT);
  assert.deepEqual(asTypography({}), TYPOGRAPHY_DEFAULT);
  assert.deepEqual(typographyStyle(null), {
    "--ym-body-size": "17px",
    "--ym-body-line": "190%",
    "--ym-body-spacing": "0em",
  });
});

test("设过就照它来；三个值各归各位", () => {
  const style = typographyStyle({
    editor_font_size: 22,
    editor_line_height: 220,
    editor_letter_spacing: 5,
  });
  assert.deepEqual(style, {
    "--ym-body-size": "22px",
    "--ym-body-line": "220%",
    "--ym-body-spacing": "0.05em",
  });
});

test("越界 / 坏值一律夹进可读范围（界面这边也夹一次）", () => {
  // 核心会夹，但界面读到的也可能来自旧库或手改的 JSON：这里再兜一次，绝不把 1000px 绑上去
  assert.equal(asTypography({ editor_font_size: 1000 }).size, 30);
  assert.equal(asTypography({ editor_font_size: 1 }).size, 12);
  assert.equal(asTypography({ editor_line_height: 3 }).line, 110);
  assert.equal(asTypography({ editor_line_height: 9999 }).line, 260);
  assert.equal(asTypography({ editor_letter_spacing: 999 }).spacing, 20);
  assert.equal(asTypography({ editor_letter_spacing: -5 }).spacing, 0);
  // 0 / null / NaN 都按"没设过"处理（0 是界面表达"回默认"的方式）
  assert.equal(asTypography({ editor_font_size: 0 }).size, TYPOGRAPHY_DEFAULT.size);
  assert.equal(asTypography({ editor_font_size: null }).size, TYPOGRAPHY_DEFAULT.size);
  assert.equal(asTypography({ editor_font_size: Number.NaN }).size, TYPOGRAPHY_DEFAULT.size);
});

test("档位表与核心的可读范围一致（改一处忘另一处，这条会红）", () => {
  const ranges = Object.fromEntries(TYPOGRAPHY_KNOBS.map((k) => [k.field, [k.min, k.max]]));
  assert.deepEqual(ranges, {
    editor_font_size: [12, 30],
    editor_line_height: [110, 260],
    editor_letter_spacing: [0, 20],
  });
  // 三个变量名不许重复，也不许写错前缀（绑不上就是"调了没反应"）
  const vars = TYPOGRAPHY_KNOBS.map((k) => k.cssVar);
  assert.equal(new Set(vars).size, vars.length);
  assert.ok(vars.every((name) => name.startsWith("--ym-body-")));
});

test("行距那个旋钮显示成倍数，不是百分比", () => {
  const line = TYPOGRAPHY_KNOBS.find((k) => k.field === "editor_line_height");
  assert.ok(line);
  assert.equal(line.format(190), "1.9");
  assert.equal(line.format(195), "1.95");
});
