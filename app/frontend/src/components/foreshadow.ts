// 伏笔那一屏的**纯展示逻辑**：状态怎么称呼、筛选项怎么算、哪一章叫什么。
//
// 单独成文件的原因与 `entity.ts` 同一条：这些都是纯函数，能单测；
// 组件只管把它们摆上去。文案一律从字典取（`foreshadow.*`）。

import { t } from "../locales/index.ts";
import type { ChapterRef, Foreshadow, ForeshadowBoard, ForeshadowState } from "../api/foreshadow.ts";

/** 三个状态的顺序（界面上筛选项的顺序）。 */
export const FORESHADOW_STATES: ForeshadowState[] = ["planted", "collected", "dropped"];

/** 状态怎么称呼；字典里没有就照原样露出来（比静默说成别的强）。 */
export function stateLabel(state: string): string {
  const key = `foreshadow.state.${state}`;
  const text = t(key);
  return text === key ? state : text;
}

/** 一个筛选项：`all` 是"全部"，其余是某一态。 */
export interface ForeshadowFilterOption {
  state: ForeshadowState | "all";
  count: number;
}

/** 面板上的筛选项：**全部**在最前，跟着三个状态（0 的也摆，好知道是 0）。 */
export function filterOptions(board: ForeshadowBoard | null): ForeshadowFilterOption[] {
  if (!board) return [{ state: "all", count: 0 }];
  return [
    { state: "all", count: board.items.length },
    { state: "planted", count: board.planted },
    { state: "collected", count: board.collected },
    { state: "dropped", count: board.dropped },
  ];
}

/** 按筛选项挑出要摆的那几条（`all` 原样返回）。 */
export function visibleItems(
  items: Foreshadow[],
  filter: ForeshadowState | "all",
): Foreshadow[] {
  if (filter === "all") return items;
  return items.filter((item) => item.state === filter);
}

/**
 * 这一章叫什么（列表上"埋于第 3 章"那一截）。
 *
 * 三种情况都说人话：查得到就用**渲染后的章名**；查不到（锚点在别的书 / 刚被删）说
 * 「第 N 章」不够诚实——那会编一个号出来，所以这里回落到"没记"，与"没填"同一条口径。
 */
export function chapterLabel(chapters: ChapterRef[], node_id: number | null): string {
  if (node_id === null) return "";
  const found = chapters.find((chapter) => chapter.id === node_id);
  return found ? t("foreshadow.at_chapter", { index: found.index }) : "";
}

/** 「埋于第 3 章」/「收于第 12 章」那一句；没有落点就返回空串（界面不摆这一截）。 */
export function anchorLabel(
  chapters: ChapterRef[],
  node_id: number | null,
  kind: "planted" | "collected",
): string {
  const chapter = chapterLabel(chapters, node_id);
  if (chapter === "") return "";
  return kind === "planted"
    ? t("foreshadow.planted_at", { chapter })
    : t("foreshadow.collected_at", { chapter });
}

/** 这条伏笔现在能走到哪些态（与核心的迁移表同源；界面据此摆按钮）。 */
export function nextStates(state: ForeshadowState): ForeshadowState[] {
  return state === "planted" ? ["collected", "dropped"] : ["planted"];
}

/** 一句话说清"这条线头从哪儿来"：埋点那一截拿不到就说"还没记埋在哪一章"。 */
export function whereLabel(chapters: ChapterRef[], item: Foreshadow): string {
  return anchorLabel(chapters, item.planted_node, "planted");
}
