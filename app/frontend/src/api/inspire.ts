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
