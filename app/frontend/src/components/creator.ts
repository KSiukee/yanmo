// 创作流面板的**纯展示逻辑**：种类怎么称呼、筛选项怎么算、关联着哪一章。
//
// 单独成文件的原因与 `question.ts` 同一条：这些都是纯函数，能单测；
// 组件只管把它们摆上去。文案一律从字典取（`creator.*`）——界面文案只有那一处来源。

import { t } from "../locales/index.ts";
import type { CreatorBoard, Fragment, FragmentKind } from "../api/fragment.ts";

/**
 * **作者能随手记的那几种**（顺序就是界面上的顺序）。
 *
 * 与核心 `FragmentKind::JOTTED` 是同一组：`question` / `answer` 归叩问那条线产，
 * 面板不建也不筛（它们有自己的处置与回头路）。口述那一档存储已经在位，
 * 写入者由口述那条链路带——这一版它只会在列表里出现，不在"记成"里选。
 */
export const JOTTED_KINDS: FragmentKind[] = ["idea", "event", "dictation"];

/** 记一条时能选的种类（口述不是键盘能"记"出来的，所以不在这一组里）。 */
export const WRITABLE_KINDS: FragmentKind[] = ["idea", "event"];

/**
 * 一种碎片怎么称呼。
 *
 * 字典里没有这一种（核心那边先长了新种类）时**照原样露出来**——
 * 屏幕上出现一个 `memo` 是一眼看得见的错，比显示成"灵感"强。
 */
export function kindLabel(kind: string): string {
  const key = `creator.kind.${kind}`;
  const text = t(key);
  return text === key ? kind : text;
}

/** 怎么记下的：只有"不是键盘敲的"才值得说一句（手打的不用特意标）。 */
export function sourceBadge(source: string): string {
  if (source === "voice") return t("creator.source.voice");
  if (source === "mixed") return t("creator.source.mixed");
  return "";
}

/** 一个筛选项：`all` 是"全部"，其余是某一种。 */
export interface FilterOption {
  kind: FragmentKind | "all";
  count: number;
}

/**
 * 面板上的筛选项：**全部**在最前，后面按 `JOTTED_KINDS` 的顺序，
 * **一条都没有的种类不摆出来**（摆一排 0 除了占地方没别的用）。
 *
 * 数字来自核心按同一种类算的计数（不是数当前这一屏），所以"全部"与各档永远对得上。
 * 「全部」**只加随手记的那几种**：核心眼下也只回这几种，但万一哪天它把问题 / 答案
 * 的计数也一起带回来，"全部 13"配着底下列出的 3 条就是一句假话。
 */
export function filterOptions(board: CreatorBoard | null): FilterOption[] {
  const counts = (board?.counts ?? []).filter((item) => JOTTED_KINDS.includes(item.kind));
  const total = counts.reduce((sum, item) => sum + item.count, 0);
  const out: FilterOption[] = [{ kind: "all", count: total }];
  for (const kind of JOTTED_KINDS) {
    const found = counts.find((item) => item.kind === kind);
    if (found && found.count > 0) out.push({ kind, count: found.count });
  }
  return out;
}

/** 按筛选项挑出要摆的那几条（`all` 原样返回）。 */
export function visibleFragments(fragments: Fragment[], filter: FragmentKind | "all"): Fragment[] {
  if (filter === "all") return fragments;
  return fragments.filter((item) => item.kind === filter);
}

/**
 * 这条碎片是不是**在当前这一章写下的**：锚点里有没有 `chapter:<当前章>`。
 *
 * 锚点的写法与核心一致（`chapter:12`）——将来按章/卷把它找回来也认这一条。
 */
export function anchoredToChapter(anchors: string[], node_id: number | null): boolean {
  return node_id !== null && anchors.includes(`chapter:${node_id}`);
}

/** 各类加起来共几条（面板标题上那个数字）。**与筛选项同一口径**，别各算一份。 */
export function totalCount(board: CreatorBoard | null): number {
  return filterOptions(board)[0].count;
}
