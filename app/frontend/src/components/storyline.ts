// 故事总纲的**纯展示逻辑**：摘要怎么截、算不算"还没写"。
//
// 单独成件的原因与 `grid.ts` 那一族同一条：这些是纯函数，能单测；
// 组件与面板只管把它们摆上去。文案一律从字典取（`grid.storyline*` / `storyline.*`）。

import { t } from "../locales/index.ts";

/** 这一段算不算"还没写"（判据只有一条：修剪后是不是空的——与核心同一条口径）。 */
export function storylineEmpty(text: string): boolean {
  return text.trim() === "";
}

/**
 * 摆在大纲表表头的那一行摘要：**取第一句有内容的话**，太长就截断。
 *
 * 为什么取第一句而不是整段：表头那一行是"让你知道写没写、写了什么"，不是阅读区——
 * 整段摊开会把表格挤下去。想看全文点一下（跳进「资料 → 总纲」）。
 */
export function storylineExcerpt(text: string, max = 60): string {
  const first = text.split("\n").find((line) => line.trim() !== "");
  if (first === undefined) return t("grid.storyline_empty");
  const trimmed = first.trim();
  return trimmed.length > max ? `${trimmed.slice(0, max)}…` : trimmed;
}
