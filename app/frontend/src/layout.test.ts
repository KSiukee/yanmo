// 布局不变量：**正文只能在编辑区里滚，不许把整页撑开**。
//
// 为什么值得专门盯着：真踩过——长章节把 CSS 网格的隐含行顶高，整个页面跟着滚，
// 顶栏与两侧栏一起飘出视野（用户原话：「长文两边的栏飘上面去了，应该保持两边不动」）。
// 这类毛病靠肉眼测不出来（短稿永远正常），只在"内容够长"时才出现，正适合机器盯着。
//
// 两条判据（读源码，简单粗暴但够用）：
// 1. `.shell__body` 里必须钉住行高 `minmax(0, 1fr)` —— 不给的话隐含行按内容高度长；
// 2. `.editor` 必须显式写 `min-height: 0` —— 网格项默认 `min-height: auto` 就是那个坑。

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));

/** 取某个选择器的规则块（`选择器 {` 到第一个 `}`）。 */
function block(file: string, selector: string): string {
  const text = readFileSync(join(here, file), "utf8");
  const start = text.indexOf(`${selector} {`);
  assert.ok(start >= 0, `${file} 里找不到 ${selector} 的规则块`);
  const end = text.indexOf("}", start);
  assert.ok(end > start, `${file} 里 ${selector} 的规则块没闭合`);
  return text.slice(start, end);
}

test("三栏骨架：正文不许把整页撑开（长章节也不许）", () => {
  const body = block("App.vue", ".shell__body");
  assert.match(
    body,
    /grid-template-rows:\s*minmax\(0,\s*1fr\)/,
    "三栏容器的行高必须钉死（grid-template-rows: minmax(0, 1fr)），否则长正文会把整页顶开",
  );
  assert.match(body, /overflow:\s*hidden/, "三栏容器要裁住溢出，别让子元素顶开整页");

  const editor = block("components/editor-pane.css", ".editor");
  assert.match(
    editor,
    /min-height:\s*0/,
    "编辑栏是网格项，必须显式写 min-height: 0（默认 auto 会被内容顶高）",
  );
});

test("滚动只发生在编辑区里（编辑栏自己不滚）", () => {
  const area = block("components/editor-pane.css", ".editor__area");
  assert.match(area, /overflow:\s*auto/, "正文区自己滚");
  assert.match(area, /min-height:\s*0/, "正文区也要压住自动最小高度");
});
