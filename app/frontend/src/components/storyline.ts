// 故事总纲的**纯展示逻辑**：这一段算不算"还没写"。
//
// 单独成件的原因与 `grid.ts` 那一族同一条：这是纯函数，能单测；组件只管摆上去。

/** 这一段算不算"还没写"（判据只有一条：修剪后是不是空的——与核心同一条口径）。 */
export function storylineEmpty(text: string): boolean {
  return text.trim() === "";
}
