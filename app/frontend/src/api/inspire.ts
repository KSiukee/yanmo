// 灵感：**一池碎片里此刻最该看哪几条**，以及把它落到哪儿去。
//
// 这一层只做"取值 ↔ 参数"的转换；引力公式、候选挑选、落点校验与留痕全在核心。
// 与创作流那条线（`fragment.ts`）同一张表、同一批碎片——只是那条线管"记 / 看 / 删"，
// 这条线管"排 / 采纳"。所以界面上下两块共用一份数据来源，不必各存一份。
//
// 与叩问那条线（`question.ts`）的分别：那边是"机制挑问题问你"，这边是"机制替你排序"。
// 两条线的落点集合**故意不同**：灵感多一档 `event`（升成一条事件，进事件池）。

import { call, COMMANDS } from "./core";
import type { FragmentKind } from "./fragment";

/** 采纳一条灵感的落点（与核心 `SparkTarget` 的稳定码一致）。 */
export type SparkTarget = "body" | "outline" | "scene" | "event";

/** 引力的拆解：四项乘数 + 总引力（界面拿它回答"为什么先看它"）。 */
export interface SparkGravity {
  total: number;
  importance: number;
  /** 记下之后的时间衰减：刚记下的最热 */
  heat: number;
  /** 与当前这一章的锚点命中程度（没有焦点时一律 1） */
  relevance: number;
  /** 没被采纳过 = 1（最高）；采纳一次掉一截，随时间回收 */
  novelty: number;
  /** 关联锚点命中了几条 */
  anchor_hits: number;
}

/** 一条灵感候选：碎片本身 + 它的引力拆解。 */
export interface Spark {
  id: number;
  kind: FragmentKind;
  body: string;
  /** 怎么记下的：`typed` / `voice` / `mixed` */
  source: string;
  /** 关联锚点（`chapter:12` 这种） */
  anchors: string[];
  importance: number;
  created_at: number;
  /** 被采纳过几次（采纳那一档的账） */
  used_count: number;
  gravity: SparkGravity;
}

/** 灵感那一屏（排好序的候选 + 落点闭集）。 */
export interface InspireBoard {
  sparks: Spark[];
  targets: SparkTarget[];
}

/** 采纳的回执：落到哪儿去了 + 采纳之后这一屏该是什么。 */
export interface AdoptReceipt {
  spark_id: number;
  target: SparkTarget;
  /** 落正文时要插进编辑器的那段字（其余落点是空串） */
  text: string;
  /** 章纲最后成了什么（没落章纲就是 null） */
  outline: string | null;
  scene_id: number | null;
  event_id: number | null;
  sparks: Spark[];
}

/**
 * 看一眼：这本书里此刻最该看的几条灵感。
 *
 * `node_id`＝现在打开着哪一章（可不给）。它**只影响排序**：跟这一章有关联的排前面。
 */
export const inspireBoard = (work_id: number, node_id: number | null = null) =>
  call<InspireBoard>(COMMANDS.inspireBoard, { work_id, node_id });

/**
 * 采纳一条：记下这本账，并按落点送出去。
 *
 * 落 `outline` / `scene` 要 `node_id`（落到哪一章）；`title` 只对场景卡有意义。
 * **落正文时字由界面先插进编辑会话**，核心不碰编辑器手里那一份。
 */
export const inspireAdopt = (
  id: number,
  target: SparkTarget,
  node_id: number | null = null,
  title = "",
) => call<AdoptReceipt>(COMMANDS.inspireAdopt, { id, target, node_id, title });

/** 丢弃一条（软删，能捞回；不动"被采纳过几次"那本账）。 */
export const inspireDiscard = (id: number, node_id: number | null = null) =>
  call<Spark[]>(COMMANDS.inspireDiscard, { id, node_id });

/** 碰撞的一头是什么（与核心 `CollisionKind` 的稳定码一致）。 */
export type CollisionKind = "spark" | "plan" | "foreshadow" | "entity";

/** 两头的关系（与核心 `CollisionRelation` 的稳定码一致）。 */
export type CollisionRelation = "together" | "apart";

/**
 * 碰撞的一头给界面看的那一份。
 *
 * `key` 是核心给的**规范标识**（`spark:12` / `plan:3`）：保存与跳过时**原样交回去**，
 * 界面不自己拼（拼错就是"认不出的碰撞对象"）。
 */
export interface CollisionSideView {
  key: string;
  kind: CollisionKind;
  /** 要嵌进引子的那段字（碎片 / 章纲 / 伏笔是正文，人物卡是名字） */
  text: string;
  /** 人物卡的备注（其余是空串） */
  note: string;
  /** 碎片那一头带着碎片本身；元素那一头是 null */
  spark: Spark | null;
}

/** 一对碰撞建议：两头 + 关系 + 分数。 */
export interface Collision {
  left: CollisionSideView;
  right: CollisionSideView;
  relation: CollisionRelation;
  /** 共用的那几条锚点（相隔时是空的） */
  shared_anchors: string[];
  score: number;
}

/** 存下一条碰撞卡的回执。 */
export interface CollisionReceipt {
  /** 新写下的那张灵感卡（双源溯源已经在它身上） */
  spark_id: number;
  /** 撞完之后这一屏该是什么（撞过的那对自动不再出现） */
  collisions: Collision[];
}

/** 看一眼碰撞：碎片与大纲元素里，哪两头值得放一起撞一下。 */
export const inspireCollisions = (work_id: number, node_id: number | null = null) =>
  call<Collision[]>(COMMANDS.inspireCollisions, { work_id, node_id });

/**
 * 存下撞出来的那一句（写成一张双源溯源的灵感卡）。
 *
 * `left` / `right` 用核心给的规范标识（`spark:12` / `plan:3`）原样交回。
 */
export const inspireCollide = (
  left: string,
  right: string,
  body: string,
  node_id: number | null = null,
) => call<CollisionReceipt>(COMMANDS.inspireCollide, { left, right, body, node_id });

/** 跳过这一对（落盘：下一次挑对还认它）。 */
export const inspireSkipCollision = (
  work_id: number,
  left: string,
  right: string,
  node_id: number | null = null,
) => call<Collision[]>(COMMANDS.inspireSkipCollision, { work_id, left, right, node_id });

/** 撤销一次跳过。 */
export const inspireUnskipCollision = (
  work_id: number,
  left: string,
  right: string,
  node_id: number | null = null,
) => call<Collision[]>(COMMANDS.inspireUnskipCollision, { work_id, left, right, node_id });
