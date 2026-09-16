// 大纲体检：**只看不说**——把对不上的地方列出来，改不改由作者。
//
// 这一层只做"取值 ↔ 参数"的转换；规则是核心的纯逻辑（`outline::rules`）。
//
// 两个口径（与核心一致）：
// - **零文案**：核心给的是规则码 + 参数 + 定位锚点，句子在界面字典（`outline.issue.*`）；
// - **含已忽略的**：`issues` 给全部（含忽略过的），`dismissed` 给指纹——
//   面板要能列出"我忽略过的"并让作者捡回来，只发没忽略的那部分就再也找不到回头路。

import { call, COMMANDS } from "./core";

/** 一处对不上的地方。 */
export interface OutlineIssue {
  /** 规则码（字典键：`outline.issue.<码>`） */
  rule: string;
  /** 定位锚点：`entity:3` / `scene:9`——点得动 */
  anchors: string[];
  /** 渲染句子用的取值（名字 / 属性键 / 缺了哪几格……） */
  params: Record<string, string>;
  /** 这条问题的身份（忽略标记认它；同一条下一轮还是它） */
  fingerprint: string;
}

/** 体检的一整屏。 */
export interface OutlineBoard {
  issues: OutlineIssue[];
  dismissed: string[];
}

/** 扫一遍这本书的大纲（**只读**：点开看一眼不该动一个字节）。 */
export const outlineScan = (work_id: number) =>
  call<OutlineBoard>(COMMANDS.outlineScan, { work_id });

/** 「这一处我知道了」：记下这条问题的指纹（幂等）。 */
export const outlineDismiss = (work_id: number, fingerprint: string) =>
  call<OutlineBoard>(COMMANDS.outlineDismiss, { work_id, fingerprint });

/** 撤销一次忽略（回头路）。 */
export const outlineUndismiss = (work_id: number, fingerprint: string) =>
  call<OutlineBoard>(COMMANDS.outlineUndismiss, { work_id, fingerprint });

/** 全部重新看一遍（"我改过设定了"）。 */
export const outlineClearDismissed = (work_id: number) =>
  call<OutlineBoard>(COMMANDS.outlineClearDismissed, { work_id });
