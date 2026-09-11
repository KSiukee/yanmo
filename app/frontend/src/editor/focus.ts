// 焦点策略：**打开一章之后，光标该不该落在正文里**。
//
// 三条分寸（合起来才是"防误触"与"接着写"的平衡）：
// 1. **历史章一律不聚焦**：打开旧章是"看"（回看、检查、找伏笔），不是"写"；
//    光标要是落在里面，随手敲一下就在正文里插进了乱字符——这类改动最难发现。
// 2. **刚新建 / 补写的章接着写**：作者刚点了「+」或「补写这一章」，意图就是要写，
//    于是跳到段末并聚焦。
// 3. **读过的最新章回原位**：有"上次读到哪"的记录就回原位（不聚焦）——
//    不能因为"这是最新章"就把他拽到段尾。
//
// 纯函数、不依赖编辑器实例：策略可以脱离界面单测，视图只负责照它执行。

export interface FocusInput {
  /** 这一章是不是**刚刚新建/补写**出来的（作者的意图就是要写） */
  fresh: boolean;
  /** 打开这一章时，库里有没有"上次读到哪"的记录 */
  had_cursor: boolean;
  /** 这一章是不是全书最后一章（跨卷按阅读顺序） */
  is_latest: boolean;
  /** 作者的偏好：接着写时跳到段末并聚焦（关掉就只是打开） */
  jump_to_end: boolean;
}

/** 打开一章之后该干什么：跳到段末并聚焦 / 把光标挪出正文 / 什么都不做。 */
export type FocusPlan = "focus-end" | "blur" | "leave";

export function focusPlan(input: FocusInput): FocusPlan {
  const write = () => (input.jump_to_end ? "focus-end" : "leave") as FocusPlan;
  if (input.fresh) return write(); // 刚新建/补写：意图明确，就是来写的
  if (!input.is_latest) return "blur"; // 历史章：看，不写
  if (input.had_cursor) return "leave"; // 读过的最新章：回到他上次的位置
  return write(); // 首次打开最新章：接着写
}
