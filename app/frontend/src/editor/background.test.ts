// 正文背景那一栏的纯规则验收：**档位认不对就回默认、一格的边长与稿纸横线同一把尺子**，
// 以及两条"只提示不挡人"的可读性提示。

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  asBgColor,
  asBgFit,
  asBgLayout,
  asBgOpacity,
  backgroundStyle,
  BG_COLORS,
  BG_FITS,
  BG_LAYOUTS,
  DEFAULT_BG_COLOR,
  DEFAULT_BG_LAYOUT,
  DEFAULT_BG_OPACITY,
  fitStyle,
  readabilityHint,
} from "./background.ts";
import { ruleStep } from "./rule.ts";

test("认不出的码 / 缺项一律回默认那一档（不做半吊子猜测）", () => {
  assert.equal(asBgColor({ editor_bg_color: "sepia" }), DEFAULT_BG_COLOR);
  assert.equal(asBgColor({}), DEFAULT_BG_COLOR);
  assert.equal(asBgColor(null), DEFAULT_BG_COLOR);
  assert.equal(asBgLayout({ editor_bg_layout: "lined" }), DEFAULT_BG_LAYOUT);
  assert.equal(asBgFit({ editor_bg_fit: "stretch" }), "cover");
  // 认得的那几档要原样认出来
  assert.equal(asBgColor({ editor_bg_color: "green" }), "green");
  assert.equal(asBgLayout({ editor_bg_layout: "dots" }), "dots");
  assert.equal(asBgFit({ editor_bg_fit: "tile" }), "tile");
});

test("不透明度：坏值回默认、越界夹住、取整（与核心写侧同一条尺子）", () => {
  assert.equal(asBgOpacity({ editor_bg_opacity: 0 }), 0, "0 是正经取值：图留着但看不见");
  assert.equal(asBgOpacity({ editor_bg_opacity: 150 }), 100);
  assert.equal(asBgOpacity({ editor_bg_opacity: -20 }), 0);
  assert.equal(asBgOpacity({ editor_bg_opacity: 33.6 }), 34);
  assert.equal(asBgOpacity({ editor_bg_opacity: Number.NaN }), DEFAULT_BG_OPACITY);
  assert.equal(asBgOpacity(null), DEFAULT_BG_OPACITY);
});

test("三档铺法都说清了 size 与 repeat 两件事", () => {
  assert.deepEqual(fitStyle("cover"), { size: "cover", repeat: "no-repeat" });
  assert.deepEqual(fitStyle("contain"), { size: "contain", repeat: "no-repeat" });
  assert.deepEqual(fitStyle("tile"), { size: "auto", repeat: "repeat" });
});

test("底纹一格的边长与稿纸横线**同一把尺子**（改字号或行距两边一起走）", () => {
  const values = { editor_font_size: 20, editor_line_height: 200 };
  assert.equal(backgroundStyle(values)["--ym-bg-step"], `${ruleStep(values)}px`);
  assert.equal(backgroundStyle(values)["--ym-bg-step"], "40px");
  // 小数位不丢（行距是百分比，乘出来常常带小数）：四舍五入会让底纹一行一行漂开
  const odd = { editor_font_size: 17, editor_line_height: 195 };
  assert.equal(backgroundStyle(odd)["--ym-bg-step"], "33.15px");
  assert.equal(backgroundStyle(odd)["--ym-bg-step"], `${ruleStep(odd)}px`);
});

test("不透明度变成图片层那一层的 alpha（0~1）", () => {
  assert.equal(backgroundStyle({ editor_bg_opacity: 35 })["--ym-bg-image-opacity"], "0.35");
  assert.equal(backgroundStyle({ editor_bg_opacity: 100 })["--ym-bg-image-opacity"], "1");
  assert.equal(backgroundStyle({ editor_bg_opacity: 0 })["--ym-bg-image-opacity"], "0");
  // 铺法也一并绑上（样式表按这两个变量铺图）
  assert.equal(backgroundStyle({ editor_bg_fit: "tile" })["--ym-bg-image-repeat"], "repeat");
  assert.equal(backgroundStyle({ editor_bg_fit: "contain" })["--ym-bg-image-size"], "contain");
});

test("可读性提示：没有图就不说；图太实说「不透明度」，有色底 + 底纹说「花」", () => {
  assert.equal(readabilityHint({}), null, "没有图就没有可提示的");
  assert.equal(
    readabilityHint({ editor_bg_image: "a.png", editor_bg_opacity: 80 }),
    "opacity",
  );
  assert.equal(
    readabilityHint({ editor_bg_image: "a.png", editor_bg_opacity: 20 }),
    null,
    "图淡淡的、底色是纸白：没什么可说的",
  );
  assert.equal(
    readabilityHint({
      editor_bg_image: "a.png",
      editor_bg_opacity: 20,
      editor_bg_color: "green",
      editor_bg_layout: "grid",
    }),
    "busy",
  );
  // 底线：**只提示，不改任何东西**（提示归提示，档位还是作者选的那一档）
  assert.equal(asBgColor({ editor_bg_color: "green" }), "green");
});

test("三张穷举表都要完整、不重不漏（面板按它们摆选项）", () => {
  assert.deepEqual([...BG_COLORS], ["paper", "cream", "green", "gray"]);
  assert.deepEqual([...BG_LAYOUTS], ["none", "grid", "dots", "grain"]);
  assert.deepEqual([...BG_FITS], ["cover", "contain", "tile"]);
});
