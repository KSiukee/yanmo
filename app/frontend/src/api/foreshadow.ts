// 伏笔：埋下的一条线头（一句话 + 埋在第几章 + 收了没有）。
//
// 这一层只做"取值 ↔ 参数"的转换；状态机的合法性、锚点校验、留痕全在核心。
//
// 三个口径（与核心一致）：
// - **"不写了"是正经结局**（不是失败，也不该再被体检念）；
// - **走一步要合法**：界面摆哪几个按钮看 `nextStates`（与核心的迁移表同源）；
// - **写错的锚点核心会拒**（不存在 / 已删 / 别的书），界面只报那一句。

import { call, COMMANDS } from "./core";

/** 一条伏笔的状态（与核心 `ForeshadowState` 的稳定码一致）。 */
export type ForeshadowState = "planted" | "collected" | "dropped";

/** 一章的名分（伏笔要挂到它上面）。 */
export interface ChapterRef {
  id: number;
  /** 渲染后的名字（`第3章` 这种） */
  title: string;
  /** 第几章（1 起） */
  index: number;
}

/** 一条伏笔。 */
export interface Foreshadow {
  id: number;
  work_id: number;
  body: string;
  /** 埋在哪一章；null = 没记（那就判不了"埋了多久"） */
  planted_node: number | null;
  /** 收在哪一章；null = 还没收 / 没记 */
  collected_node: number | null;
  state: ForeshadowState;
  note: string;
  created_at: number;
  updated_at: number;
}

/** 一整屏：伏笔 + 按状态分的数字 + 章的名分。 */
export interface ForeshadowBoard {
  items: Foreshadow[];
  planted: number;
  collected: number;
  dropped: number;
  chapters: ChapterRef[];
}

/** 一条伏笔现在能走到哪些态（与核心的迁移表同源）。 */
export function nextStates(state: ForeshadowState): ForeshadowState[] {
  return state === "planted" ? ["collected", "dropped"] : ["planted"];
}

/** 列这本书的伏笔（埋着的排前面）。 */
export const foreshadowList = (work_id: number) =>
  call<ForeshadowBoard>(COMMANDS.foreshadowList, { work_id });

/** 记一条（状态从「埋着」开始）。 */
export const foreshadowCreate = (
  work_id: number,
  body: string,
  planted_node: number | null,
  note = "",
) => call<ForeshadowBoard>(COMMANDS.foreshadowCreate, { work_id, body, planted_node, note });

/** 改一条（正文 / 埋点 / 备注；**状态不动**）。 */
export const foreshadowUpdate = (
  id: number,
  body: string,
  planted_node: number | null,
  note = "",
) => call<ForeshadowBoard>(COMMANDS.foreshadowUpdate, { id, body, planted_node, note });

/** 走一步状态（收到哪一章可给可不给）。 */
export const foreshadowMove = (id: number, to: ForeshadowState, collected_node: number | null) =>
  call<ForeshadowBoard>(COMMANDS.foreshadowMove, { id, to, collected_node });

/** 删一条（软删）。 */
export const foreshadowDelete = (id: number) =>
  call<ForeshadowBoard>(COMMANDS.foreshadowDelete, { id });
