// 「计划 vs 实际」那一屏的**纯展示逻辑**：三档怎么说、能补谁、补完算哪一档。
//
// 单独成文件的原因与 `grid.ts` 同一条：这些都是纯函数，能单测；组件只管摆上去。
// 文案一律从字典取（`actual.*`）——界面文案只有那一处来源。

import type { ChapterActualDto, OutlineActualPageDto, OutlineSnapshotDto } from "../api/outline";

/** 三档的说法键（字典 `actual.state_<码>`）。 */
export function stateLabelKey(state: string): string {
  return `actual.state_${state}`;
}

/**
 * 这一章能补谁：**正文里出现、计划里没有**的那几张卡。
 *
 * 只补不删是产品定案：计划里挂了、正文里没认到的人（`missing`）**不进这个名单**——
 * 删计划比加计划危险得多，那一类只提示。
 */
export function alignTargets(chapter: ChapterActualDto): number[] {
  return chapter.extra.map((ref) => ref.entity_id);
}

/**
 * 补完之后这一章算哪一档。
 *
 * 与核心 `outline::actual::chapter_state` 同一条规则（还有没认到的人就是偏离）。
 * 这里算一次只是为了"点完立刻看见变化"，**下一次全书对一遍才是准的**。
 */
export function stateAfterAlign(chapter: ChapterActualDto): string {
  if (!chapter.has_body) {
    return chapter.state;
  }
  return chapter.missing.length > 0 ? "deviated" : "written";
}

/** 这一章最近那份大纲留底（有才给：界面拿它决定"撤销"露不露）。 */
export function undoRef(
  page: OutlineActualPageDto | null,
  node_id: number,
): OutlineSnapshotDto | null {
  return page?.undoable.find((item) => item.node_id === node_id) ?? null;
}
