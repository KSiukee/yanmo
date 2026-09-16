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

/** 大纲表里的一行：一个节点（卷 / 章 / 节 / 场景卡）带上该有的列。 */
export interface OutlineRowDto {
  node_id: number;
  /** `volume` / `chapter` / `section` / `piece` / `scene` */
  kind: string;
  parent_id: number | null;
  /** 缩进层级（0 = 卷那一层） */
  depth: number;
  /** 渲染后的名字（空串＝还没起名） */
  title: string;
  /** 这一章的"一句话"（章纲） */
  summary: string;
  word_count: number;
  has_body: boolean;
  /** 四格（没填过就是四个空串） */
  fields: { pov: string; goal: string; conflict: string; outcome: string };
  /** 这一章埋着还没收的伏笔有几条 */
  planted_open: number;
  /** 这一章收掉的伏笔有几条 */
  collected: number;
}

/** 伏笔那一列要的两个数（`ForeshadowCount` 只是别名，读起来顺一点）。 */
export type ForeshadowCount = Pick<OutlineRowDto, "planted_open" | "collected">;

/** 大纲表要读的那一屏（**整棵树铺平**，一次给全）。 */
export const outlineRows = (work_id: number) =>
  call<OutlineRowDto[]>(COMMANDS.outlineRows, { work_id });

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
