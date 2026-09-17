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
  /** 这一段出场的人物（空表＝还没挂过；名字是刚读到的） */
  cast: CastMember[];
}

/** 名单里的一个人：设定卡的 id + 它**现在**的名字（卡改名，下次读就是新名）。 */
export interface CastMember {
  entity_id: number;
  name: string;
}

/** 伏笔那一列要的两个数（`ForeshadowCount` 只是别名，读起来顺一点）。 */
export type ForeshadowCount = Pick<OutlineRowDto, "planted_open" | "collected">;

/** 大纲表要读的那一屏（**整棵树铺平**，一次给全）。 */
export const outlineRows = (work_id: number) =>
  call<OutlineRowDto[]>(COMMANDS.outlineRows, { work_id });

/** 交上去的一格（界面把粘进来的那一片拆好格，核对在核心）。 */
export interface OutlineCellPayload {
  node_id: number;
  /** 稳定码：`summary` / `pov` / `goal` / `conflict` / `outcome` */
  column: string;
  value: string;
}

/**
 * 把一片粘进来的格子**一次写进库**，回这本书最新那一屏。
 *
 * 一片一次事务：里面有一格落不了，整片都不落（**绝不留下粘了一半的表**）。
 */
export const outlinePasteCells = (work_id: number, cells: OutlineCellPayload[]) =>
  call<OutlineRowDto[]>(COMMANDS.outlinePasteCells, { work_id, cells });

/** 换掉一段的出场人物（**整份覆盖**），回这一段最新那一份名单。 */
export const outlineSetCast = (node_id: number, entity_ids: number[]) =>
  call<CastMember[]>(COMMANDS.outlineSetCast, { node_id, entity_ids });

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

/** 正文里出现的一张卡（给界面显示：是哪张卡、命中哪个叫法）。 */
export interface ActualCastRef {
  entity_id: number;
  name: string;
  /** 命中的那个叫法（可能是别称） */
  matched: string;
}

/** 一条"像被点到了"的伏笔（**候选**，不是结论）。 */
export interface ActualForeshadowHit {
  foreshadow_id: number;
  body: string;
  /** 命中的片段（让作者看得见"凭什么说像"） */
  fragments: string[];
  fragments_total: number;
}

/** 一章的「计划 vs 实际」。 */
export interface ChapterActualDto {
  node_id: number;
  title: string;
  /** `unwritten` / `written` / `deviated` */
  state: string;
  has_body: boolean;
  planned: ActualCastRef[];
  matched: ActualCastRef[];
  /** 正文里出现、计划里没有的（可以一键补进计划） */
  extra: ActualCastRef[];
  /** 计划里挂了、正文里没认到的（只提示，绝不自动撤） */
  missing: ActualCastRef[];
  foreshadows: ActualForeshadowHit[];
}

/** 一份大纲留底的摘要（"这一章能不能撤销上一次对齐"认它）。 */
export interface OutlineSnapshotDto {
  id: number;
  node_id: number;
  note: string;
  created_at: number;
}

/** 「计划 vs 实际」那一屏的一页。 */
export interface OutlineActualPageDto {
  chapters: ChapterActualDto[];
  /** 这一屏之外还有几章没对到（书太大时才有） */
  truncated: number;
  undoable: OutlineSnapshotDto[];
}

/** 全书对一遍（只读；要读全部正文做字面匹配，所以是一次显式动作）。 */
export const outlineActuals = (work_id: number) =>
  call<OutlineActualPageDto>(COMMANDS.outlineActuals, { work_id });

/** 对齐的回执：补完的名单 + 这次留下的底（有底才谈得上撤销）。 */
export interface AlignReceiptDto {
  cast: CastMember[];
  added: number;
  snapshot_id: number | null;
}

/** 把正文里出现、计划里没有的人**补进**这一章的出场人物（只补不删）。 */
export const outlineAlignCast = (node_id: number, entity_ids: number[]) =>
  call<AlignReceiptDto>(COMMANDS.outlineAlignCast, { node_id, entity_ids });

/** 撤销上一次对齐（把这一章的大纲放回最近那份留底）。 */
export const outlineAlignUndo = (node_id: number) =>
  call<CastMember[]>(COMMANDS.outlineAlignUndo, { node_id });
