// 章长那一屏：这本书每一章多少字、哪一章偏长（**只读**）。
//
// 两个口径（与核心一致）：
// - **判据不在界面**：中位数、样本门槛、"超过中位数的几倍"都由核心算好
//   （`question::rhythm`，与叩问那条 `rhythm.chapter_very_long` **同一个判据**）——
//   界面只说它给的事实，绝不自己再判一次"长不长"；
// - **记号由核心给**：`chapters[].fingerprint` 是"这一章以后不再提"要写的那条原文，
//   界面照抄着交给 `outlineDismiss`（identity 的写法只有核心那一处，界面不自己拼）。
//
// 「就这样写」**没有第二条命令**：忽略记账本来就是大纲体检那一族（`outline_dismiss`，
// 按书分、与体检同一把记号），这一条链不新增任何存储。

import { call, COMMANDS } from "./core";
import { outlineDismiss } from "./outline";

/** 一章的一行事实。 */
export interface ChapterLengthItem {
  node_id: number;
  /** 这一章的字符数（与判据同一口径：`nodes.char_count`） */
  chars: number;
  /** 偏长：超过"中位数 × 倍数"（样本不足时一律 `false`） */
  long: boolean;
  /** 作者说过「就这样写」——这一章以后不再提 */
  dismissed: boolean;
  /** "不再提"那条记号的**原文**：要写就照抄它 */
  fingerprint: string;
}

/** 这一本书的章长分布 + 逐章判定。 */
export interface ChapterLengthBoard {
  /** 中位数（"这本书中位 {median} 字"念的就是它） */
  median: number;
  /** 判据用的倍数（超过中位数的几倍算特别长） */
  ratio: number;
  /** 样本门槛（少于这么多章不评） */
  min_samples: number;
  /** 样本够不够：不够 ⇒ 每一章的 `long` 都是 `false` */
  judged: boolean;
  chapters: ChapterLengthItem[];
}

/** 这本书的章长分布（**只读**：点开看一眼不该动一个字节）。 */
export const chapterLengths = (work_id: number) =>
  call<ChapterLengthBoard>(COMMANDS.chapterLengths, { work_id });

/**
 * 章长提醒要的那两件事：读一眼分布 / 把「就这样写」记进账。
 *
 * 形态与 `editor/chapter-length.ts` 的 `ChapterLengthTransport` 一致（结构化对上就行）。
 * 为什么摆在这一层：**只有 `api/` 下的文件可以 import Tauri**，而编辑器那一侧是纯逻辑——
 * 它自己不 import 通道，单测因此能直接喂一个假通道。
 */
export const chapterLengthTransport = {
  report: (work_id: number) => chapterLengths(work_id),
  keep: async (work_id: number, fingerprint: string): Promise<void> => {
    await outlineDismiss(work_id, fingerprint);
  },
};
