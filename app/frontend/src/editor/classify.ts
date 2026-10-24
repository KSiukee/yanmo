// 分类归属那一栏的**纯规则**（体裁 / 标签）：常用体裁清单、把作者打的那几行读成标签、
// 以及「按主题」排序的判据。
//
// 与核心的分工：**归一与上限在核心**（`model::tags`，写进去的一定是归一过的那一份），
// 这里只做两件界面自己的事：① 给常用体裁一列候选（**开放取值**，选不中自己打）；
// ② 排序时怎么把一屏篇目摆成"一撮一撮"。
//
// 与集内另两档排序（`collection.ts` 的按写成先后 / 按字数）分开成件的理由：
// 那两档只看每一篇自己的数与时间，这一档要看**作者填的分类**，判据不是一回事。

import type { ArticleEntry } from "../api/collection";

/**
 * 常用体裁：**只是候选，不是闭集**。
 *
 * 这一列一个都不认也照样开工——作者打什么存什么（核心不设闭集，见 `db::migrations_v20`），
 * 它唯一的用处是让"大众体裁"不必手打、不会拼错。加一条不必改结构，改这里就够。
 */
export const GENRE_PRESETS = [
  "shelf.genre_preset_scifi",
  "shelf.genre_preset_fantasy",
  "shelf.genre_preset_mystery",
  "shelf.genre_preset_romance",
  "shelf.genre_preset_wuxia",
  "shelf.genre_preset_urban",
  "shelf.genre_preset_history",
  "shelf.genre_preset_essay",
  "shelf.genre_preset_prose",
  "shelf.genre_preset_poetry",
  "shelf.genre_preset_script",
  "shelf.genre_preset_documentary",
] as const;

/** 常用体裁的字典键（界面拿它查出中文；核心零文案，这里也不写死汉字）。 */
export function genreLabelKey(key: string): string {
  return key;
}

/**
 * 把作者打的那一栏读成标签清单：**一行一个**，去首尾空白、丢掉空行、去掉重复，保留先后。
 *
 * 与核心 `model::tags::parse_tags` 同一条口径，但这里**不做上限判断**：
 * 超限由核心在写的那一下如实拒（错误码带着"最多几个"），界面不预先替它拦——
 * 拦在这里，作者就看不到核心那句说清了的报错。
 */
export function parseTags(raw: string): string[] {
  const out: string[] = [];
  for (const line of raw.split(/\r?\n|\r/)) {
    const tag = line.trim();
    if (!tag || out.includes(tag)) continue;
    out.push(tag);
  }
  return out;
}

/** 标签清单写回一栏文本（一行一个）——与 `parseTags` 是一对。 */
export function joinTags(tags: string[]): string {
  return tags.join("\n");
}

/** 这一篇的**主题**：作者写的第一个标签（没写标签就是空串）。 */
export function themeOf(entry: Pick<ArticleEntry, "tags">): string {
  return parseTags(entry.tags)[0] ?? "";
}

/** 一篇的体裁（去空白；库里已经是归一过的，这里只是兜一道）。 */
export function genreOf(entry: Pick<ArticleEntry, "genre">): string {
  return entry.genre.trim();
}

/**
 * 「按主题」摆这一屏：**先按体裁聚，组内再按各篇自己的主题**，都空的排最后。
 *
 * 判据（都是**确定性的**，同一屏摆两次结果一样）：
 * 1. 体裁：有体裁的在前，按体裁名升序；**没填体裁的自成一撮、排在最后**；
 * 2. 组内：按主题（第一个标签）升序；**没写标签的一小撮也排在组尾**；
 * 3. 其余保持**现在这个顺序**（排序是稳定的）——作者手拖过的先后不会被无谓打乱。
 *
 * ★ 一条边界如实说：这一档**不像「按主题」三个字听起来那样"聪明"**——
 * 它不猜篇与篇之间有什么关系，只用作者自己填的体裁与标签把相似的摆到一起。
 * 一个标签都没填时，它等价于"按体裁聚一聚"；体裁也全空时，它**一篇文章都不动**。
 */
export function sortByTheme(entries: ArticleEntry[]): ArticleEntry[] {
  const key = (value: string) => [value === "" ? 1 : 0, value] as const;
  const rank = (entry: ArticleEntry) => {
    const [genreEmpty, genre] = key(genreOf(entry));
    const [themeEmpty, theme] = key(themeOf(entry));
    return { genreEmpty, genre, themeEmpty, theme };
  };
  return entries
    .map((entry, index) => ({ entry, index, rank: rank(entry) }))
    .sort((a, b) => {
      if (a.rank.genreEmpty !== b.rank.genreEmpty) return a.rank.genreEmpty - b.rank.genreEmpty;
      if (a.rank.genre !== b.rank.genre) return a.rank.genre < b.rank.genre ? -1 : 1;
      if (a.rank.themeEmpty !== b.rank.themeEmpty) return a.rank.themeEmpty - b.rank.themeEmpty;
      if (a.rank.theme !== b.rank.theme) return a.rank.theme < b.rank.theme ? -1 : 1;
      return a.index - b.index; // 并列保持现在这个先后
    })
    .map((item) => item.entry);
}
