// 书架与作品信息（新建/改名/语言/分类/删除/导出）。

import { call, COMMANDS } from "./transport";

/** 书架的一行：作品 + 它的规模。 */
export interface ShelfEntry {
  id: number;
  /** 「novel / collection / article」——界面只用来显示，不假设行为差异 */
  kind: string;
  title: string;
  /** 作品简介（作者手填，空串＝没写过）：投稿包的大纲要用它 */
  summary: string;
  /** **体裁**（空串＝没填过；开放取值，核心不设闭集）——作品表单回填这一格 */
  genre: string;
  /** **标签**（一行一个，空串＝一个都没填）——集内「按主题」排序也看它 */
  tags: string;
  /** 这本书里章的个数（单篇文章是 0，此时只报字数） */
  chapters: number;
  /** 字数合计（按词口径） */
  word_count: number;
  /** 同上，逐字（含标点） */
  char_count: number;
  /** 同上，逐字（不含标点） */
  chars_no_punct: number;
  /** 最近打开（unix 毫秒）；从没打开过是 null */
  opened_at: number | null;
  created_at: number;
  updated_at: number;
}

/** 一次导出的回执。 */
export interface ExportAck {
  /** 导到哪个文件夹（界面只展示，不碰文件系统） */
  path: string;
  files: number;
  /** 顺手清掉了几个上次导出留下的旧文件 */
  removed: number;
}

/** 书架：最近打开的书在前，带每本书的章数与字数。 */
export const listShelf = () => call<ShelfEntry[]>(COMMANDS.listShelf);

/** 一本书的一眼信息：类型 / 标题 / 语言 / 落定后的字数口径（顺手记一次"打开"）。 */
export interface WorkBrief {
  id: number;
  /** novel / article / collection */
  kind: string;
  title: string;
  language: string;
  /** chars / chars_no_punct / words */
  word_caliber: string;
}

/** 切作品之前先问这一眼：**集没有正文**，切到集不该去开编辑器目标（那会补一章）。 */
export const workBrief = (work_id: number) =>
  call<WorkBrief>(COMMANDS.workBrief, { work_id });

/** 新建一本书，返回它的 id（体裁/标签留空＝先不填，之后在「编辑作品」里补）。 */
export const createWork = (kind: string, title: string) =>
  call<number>(COMMANDS.createWork, { kind, title });

/** 给书改名。 */
export const renameWork = (work_id: number, title: string) =>
  call<void>(COMMANDS.renameWork, { work_id, title });

/** 改作品语言的回执：改完的语言 + **落定后的字数口径**（界面照着刷新那个数字）。 */
export interface WorkLanguageAck {
  work_id: number;
  language: string;
  word_caliber: string;
}

/** 写分类归属（体裁 + 标签）的回执：取的是**归一过、库里真有的那一份**。
 *
 *  界面填的原样（可能带空格、带重复、带空行）不一定是库里那一份，所以写完照这份覆盖输入框。 */
export interface WorkClassifyAck {
  work_id: number;
  genre: string;
  /** 标签，**一行一个** */
  tags: string;
}

/** 写体裁与标签（人填；超限由核心如实拒，空串＝没填过）。 */
export const setWorkClassify = (work_id: number, genre: string, tags: string) =>
  call<WorkClassifyAck>(COMMANDS.setWorkClassify, { work_id, genre, tags });

/** 改作品语言（zh / en / ja）——字数默认口径跟它走；返回**写完回读**的两个码。 */
export const setWorkLanguage = (work_id: number, language: string) =>
  call<WorkLanguageAck>(COMMANDS.setWorkLanguage, { work_id, language });

/** 删掉一本书（软删除，正文与历史都留着）。 */
export const deleteWork = (work_id: number) => call<void>(COMMANDS.deleteWork, { work_id });

/** 把一本书导出成 txt（分章）或 json（单文件）；同样的内容不会重复写。 */
export const exportWork = (work_id: number, format: string) =>
  call<ExportAck>(COMMANDS.exportWork, { work_id, format });

/** 写作品简介（投稿包的大纲要用它）。 */
export const setWorkSummary = (work_id: number, summary: string) =>
  call<void>(COMMANDS.setWorkSummary, { work_id, summary });
