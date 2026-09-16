// 创作流碎片：作者自己记下的东西（灵感速记 / 事件 / 口述段落）。
//
// 这一层只做"取值 ↔ 参数"的转换；种类闭集、软删与捞回、留痕全在核心。
//
// 三个口径（与核心一致，界面上也照这么说）：
// - **与叩问同住一张表**：问题卡、答案、灵感、事件是一条存储上的不同种类——
//   所以它们共享同一套元数据（怎么记下的 / 多新鲜 / 用没用过 / 关联着哪一章）；
// - **记下来不打断**：写一条碎片没有别的副作用（不动问题状态、不动正文）；
// - **删只是打时间戳**：捞回就是把时间戳抹掉，所以"撤销"是能兑现的。

import { call, COMMANDS } from "./core";

/** 碎片的种类（与核心 `FragmentKind` 的稳定码一致）。 */
export type FragmentKind = "question" | "answer" | "idea" | "event" | "dictation";

/** 一条碎片（面板读到的统一形状）。 */
export interface Fragment {
  id: number;
  work_id: number;
  kind: FragmentKind;
  body: string;
  /** 怎么记下的：`typed` / `voice` / `mixed`（与答案同一列、同一闭集）。 */
  source: string;
  /** 关联锚点（`chapter:12` 这种）。 */
  anchors: string[];
  /** 从哪张问题卡勾出来的（自己随手记的没有）。 */
  derived_from: number | null;
  created_at: number;
}

/** 一种碎片有几条（面板上那些筛选项的数字）。 */
export interface FragmentCount {
  kind: FragmentKind;
  count: number;
}

/** 创作流面板一屏要的全部数据（列表 + 各筛选项的数字，一次给全免得前后对不上）。 */
export interface CreatorBoard {
  fragments: Fragment[];
  counts: FragmentCount[];
}

/**
 * 记一条碎片。
 *
 * `kind` 只认作者自己记的那几种（`idea` / `event` / `dictation`）——问题卡与答案归叩问，
 * 从这儿建会被核心当场拒（`fragment.kind_not_jotted`）。
 * `anchors` 是关联锚点：面板把「是在这一章写的」记成 `chapter:<章 id>`。
 */
export const fragmentAdd = (
  work_id: number,
  kind: FragmentKind,
  body: string,
  source: string,
  anchors: string[] = [],
) => call<Fragment>(COMMANDS.fragmentAdd, { work_id, kind, body, source, anchors });

/** 看一眼碎片池（不写库）。 */
export const fragmentBoard = (work_id: number) =>
  call<CreatorBoard>(COMMANDS.fragmentBoard, { work_id });

/** 删掉一条（**软删**：库里还留着，随时能捞回）。 */
export const fragmentDelete = (id: number) =>
  call<CreatorBoard>(COMMANDS.fragmentDelete, { id });

/** 捞回刚删掉的那一条（重复点也不算错）。 */
export const fragmentRestore = (id: number) =>
  call<CreatorBoard>(COMMANDS.fragmentRestore, { id });
