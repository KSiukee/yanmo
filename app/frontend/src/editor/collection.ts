// 短篇集的**纯规则**：集进度、攒集提示的阈值、上移/下移、一键建集的排序。
//
// 与 `api/collection.ts` 分工：那边碰 Tauri（类型 + 命令封装），这边只认类型——
// 所以这一份能脱离界面单测。
//
// 两条口径：
// - **目标是参考线，不是判据**：「够 N 篇」「到多少字」只影响那句进度怎么显示；
// - **字数按当前口径比**：目标填的是哪个口径，就和那个口径的当前值比（作者看到的是同一个数）。

import type { ArticleEntry, CollectionView } from "../api/collection";

/**
 * 「够 N 篇就能编成一个集了」的那个 N。
 *
 * **是提示阈值，不是判据**（设计里那句"够 N 篇时提示可以编成一个集了"）：
 * 没到也照样能建集，到了也只是**安安静静提一句**，不做成打卡或待办。
 */
export const LOOSE_ENOUGH = 5;

/** 一条进度：现在有多少、目标多少、够没够。 */
export interface Progress {
  have: number;
  want: number | null;
  done: boolean;
}

/** 篇数进度。 */
export function pieceProgress(view: Pick<CollectionView, "pieces" | "target_pieces">): Progress {
  const want = view.target_pieces;
  return { have: view.pieces, want, done: want !== null && view.pieces >= want };
}

/** 字数进度（按当前字数口径比）。 */
export function wordProgress(
  view: Pick<CollectionView, "words" | "chars" | "chars_no_punct" | "target_words">,
  caliber: string,
): Progress {
  const have = viewCount(view, caliber);
  const want = view.target_words;
  return { have, want, done: want !== null && have >= want };
}

/** 还没归集的散篇够数了吗（攒集提示就靠它）。 */
export function nudgeReady(loose: number): boolean {
  return loose >= LOOSE_ENOUGH;
}

/** 集的汇总按当前口径取哪个数。 */
function viewCount(
  view: Pick<CollectionView, "words" | "chars" | "chars_no_punct">,
  caliber: string,
): number {
  if (caliber === "words") return view.words;
  if (caliber === "chars_no_punct") return view.chars_no_punct;
  return view.chars;
}

/** 一篇的字数按当前口径取哪个数（与集的汇总同一个数法）。 */
function entryCount(entry: ArticleEntry, caliber: string): number {
  if (caliber === "words") return entry.word_count;
  if (caliber === "chars_no_punct") return entry.chars_no_punct;
  return entry.char_count;
}

/** 把第 `from` 位移到第 `to` 位（上移 / 下移用它）；越界或原地不动就原样返回。 */
export function moveId(ids: number[], from: number, to: number): number[] {
  if (from < 0 || from >= ids.length || to < 0 || to >= ids.length || from === to) return ids;
  const out = ids.slice();
  const [moved] = out.splice(from, 1);
  out.splice(to, 0, moved);
  return out;
}

/** 一键建集的默认排序：**最后动过的排前面**（刚写完的更像要收进这一本）。 */
export function byTime(entries: ArticleEntry[]): ArticleEntry[] {
  return entries.slice().sort((a, b) => b.updated_at - a.updated_at);
}

/** 另一条排序：**字多的排前面**（按当前口径）。 */
export function byLength(entries: ArticleEntry[], caliber: string): ArticleEntry[] {
  return entries.slice().sort((a, b) => entryCount(b, caliber) - entryCount(a, caliber));
}

/** 篇目那一行的字数（按当前口径，与集汇总同一个数法）。 */
export function entryWords(entry: ArticleEntry, caliber: string): number {
  return entryCount(entry, caliber);
}

/** 集内排序的两种走法（**按主题**那种要先有体裁/标签，属于分类维度那一摊，不在这里）。 */
export type SortMode = "written" | "length";

/**
 * 集内一键排序：按给定走法排一排，**不动原来那份**。
 *
 * - `written`：写成先后（早的在前）——集子的目录要的是这个，所以用 `created_at`
 *   而不是「最后动过」（刚改过的那篇不该窜到目录头上去）；
 * - `length`：字多的在前（按当前口径，与集汇总同一个数法）。
 *
 * 并列时保持现在的相对先后（排序是稳定的）：手拖过的顺序不会被无谓打乱。
 */
export function sortEntries(entries: ArticleEntry[], mode: SortMode, caliber: string): ArticleEntry[] {
  if (mode === "length") return byLength(entries, caliber);
  return entries.slice().sort((a, b) => a.created_at - b.created_at);
}

/** 排完之后的 id 清单（重排那条命令要的就是它）。 */
export function sortedIds(entries: ArticleEntry[], mode: SortMode, caliber: string): number[] {
  return sortEntries(entries, mode, caliber).map((entry) => entry.id);
}
