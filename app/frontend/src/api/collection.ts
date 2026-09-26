// 短篇集：一屏读数、加 / 拿掉 / 重排、定目标、一键建集。
//
// 这一层只做"取值 ↔ 参数"的转换；`集只是引用`这条口径在核心：
// 加、拿掉、重排都只动引用表，**从不碰正文**——所以这里也没有"写正文"这种口子。
//
// 三个口径（与核心一致）：
// - **一篇可属多个集**：同一篇稿子只写一次，年度自选与科幻集可以都要它；
// - **拿掉不删稿**：只是不在这本集子里了，书架与稿件台上照旧；
// - **软删的集不算集**：集进了回收站，它引用的篇目重新回到"还没归集的散篇"里。

import { call, COMMANDS } from "./core";

/** 集里的一篇（"还没归集的散篇"用的是同一张形状）。 */
export interface ArticleEntry {
  id: number;
  title: string;
  /** 这一篇的流转状态（七态稳定码，见 `api/manuscript.ts`） */
  status: string;
  word_count: number;
  char_count: number;
  chars_no_punct: number;
  /** 这一篇什么时候写成的（集内「按时间排」用它） */
  created_at: number;
  updated_at: number;
}

/** 一个集的一屏：篇目（按集内顺序）+ 目标 + 现有规模。 */
export interface CollectionView {
  id: number;
  title: string;
  entries: ArticleEntry[];
  /** 目标篇数（null = 没设过） */
  target_pieces: number | null;
  /** 目标总字数（null = 没设过） */
  target_words: number | null;
  pieces: number;
  words: number;
  chars: number;
  /** 逐字（不含标点）——界面按作品字数口径挑一格显示 */
  chars_no_punct: number;
}

/** 一个集的一屏（篇目按集内顺序）。 */
export const collectionView = (work_id: number) =>
  call<CollectionView>(COMMANDS.collectionView, { work_id });

/** 往集里加一篇（追加在末尾；已经在里面就是空操作）。 */
export const collectionAdd = (work_id: number, article_id: number) =>
  call<CollectionView>(COMMANDS.collectionAdd, { work_id, article_id });

/** 从集里拿掉一篇（只去引用，不碰正文）。 */
export const collectionRemove = (work_id: number, article_id: number) =>
  call<CollectionView>(COMMANDS.collectionRemove, { work_id, article_id });

/** 按给定顺序重排（名单与集里对不上会被核心拒绝，顺序原样不动）。 */
export const collectionReorder = (work_id: number, article_ids: number[]) =>
  call<CollectionView>(COMMANDS.collectionReorder, { work_id, article_ids });

/** 定集目标（篇数 / 总字数；null 或 ≤0 = 清掉那一项）。 */
export const collectionSetTarget = (
  work_id: number,
  pieces: number | null,
  words: number | null,
) => call<CollectionView>(COMMANDS.collectionSetTarget, { work_id, pieces, words });

/** 一键建集：建一个集，把这几篇按给定顺序装进去；回读新集的这一屏。 */
export const collectionCreate = (title: string, article_ids: number[]) =>
  call<CollectionView>(COMMANDS.collectionCreate, { title, article_ids });

/** 还没归集的散篇（攒集提示与"往集里加一篇"那张清单都用它）。 */
export const collectionLooseArticles = () =>
  call<ArticleEntry[]>(COMMANDS.collectionLooseArticles);

/** 这一篇属于哪几个（活着的）集。 */
export const collectionOf = (article_id: number) =>
  call<number[]>(COMMANDS.collectionOf, { article_id });

/** 集导出的两种格式（与核心 `CollectionFormat` 的稳定码一致）。 */
export type CollectionFormat = "collection_docx" | "collection_txt";

/** 一次导出的回执（与编译那一族同一个形状）。 */
export interface CollectionExportAck {
  /** 落在哪个文件夹（界面只展示） */
  path: string;
  files: { path: string; bytes: number }[];
  /** 顺手清掉了几个上次留下的旧产物 */
  removed: number;
}

/** 把一个集按集内顺序合成一份成品（目录 + 每篇各起一页）。 */
export const compileCollection = (collection_id: number, format: CollectionFormat) =>
  call<CollectionExportAck>(COMMANDS.compileCollection, { collection_id, format });

/** 打开集导出产物所在的目录。 */
export const compileCollectionOpenFolder = (collection_id: number, format: CollectionFormat) =>
  call<void>(COMMANDS.compileCollectionOpenFolder, { collection_id, format });
