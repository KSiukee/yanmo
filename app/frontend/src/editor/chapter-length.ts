// 章长提醒（防爆章）：**与用眼提醒共用同一条安静出口**——非阻塞、一次一句、说完就走。
//
// 它跟用眼提醒是同一族的第二个内容：**只在作者自己要停下来的时刻说**（切章 / 切书 / 关窗前
// 那一下，接线在 `editor/break-reminder.ts` 那三个停下点，不另起钩子），写作途中一句都不说。
//
// 四条纪律（任务口径，一条都不许松）：
//
// - **判据不在界面**：中位数、样本门槛、"超过中位数的几倍"都由核心那一处算
//   （`question::rhythm`，与叩问那条 `rhythm.chapter_very_long` **同一个判据**）——
//   这里只挑"刚离开的那一章"有没有那一行事实，绝不自己再判一次“长不长”；
// - **只报事实、不评判**：条上那句话是"这一章 4700 字，这本书中位 2000 字（2.4 倍）
//   ——是故意的吗？"——**末尾是问句，不是命令**，也不说"该拆章"；
// - **答复两态**：「就这样写」= 这一章以后不再提（落进既有的忽略记账，不新建存储）；
//   「知道了」= 只关这一次（不记账，下次停下点还超长会再提一次）；
// - **读不出来一声不响**：宁可少说一句，也不拿猜出来的数字开口（与"读不到偏好就不抢
//   焦点"同一条安全边）。
//
// 本文件是**纯逻辑 + 注入通道**：它不认识 Vue 组件、也不 import Tauri（单测喂假通道）。
// 唯一被组件读的是模块级那份 `board`（目录里那一行角标）。

import { ref, type Ref } from "vue";

import type { ChapterLengthBoard, ChapterLengthItem } from "../api/chapter-length.ts";
import { t } from "../locales/index.ts";

/** 要念的那一句里的**事实**（界面只念事实，不做判断）。 */
export interface ChapterLengthTip {
  /** 这一章属于哪本书（按「就这样写」时要带上它） */
  work_id: number;
  node_id: number;
  /** 这一章多少字 */
  chars: number;
  /** 这本书的中位数 */
  median: number;
  /** 实际倍数（这一章是中位数的几倍，一位小数） */
  times: number;
  /** "以后不再提"那条记号的**原文**（核心给的，界面照抄，不自己拼） */
  fingerprint: string;
}

/** 最近一次读到的分布（**模块级一份**：章长提醒与目录角标说的是同一件事）。 */
const board = ref<ChapterLengthBoard | null>(null);

/** 目录角标读它（偏长的章要在那一行标一下）。 */
export function chapterLengthBoard(): Ref<ChapterLengthBoard | null> {
  return board;
}

/**
 * 这一章挑得出那句话吗——**唯一的闸门**（样本不足、不偏长、他已经说过「就这样写」都在这里
 * 拦掉；`median <= 0` 是"没有中位数可念"，不是第二个判据）。
 */
function speakable(
  value: ChapterLengthBoard | null,
  node_id: number | null,
): { report: ChapterLengthBoard; item: ChapterLengthItem } | null {
  if (value === null || node_id === null || value.median <= 0) return null;
  const item = value.chapters.find((row) => row.node_id === node_id);
  if (!item || !item.long || item.dismissed) return null;
  return { report: value, item };
}

/** 这一章该不该标"偏长"（目录角标用；与停下点那一句**同一个闸门**）。 */
export function isLongChapter(value: ChapterLengthBoard | null, node_id: number | null): boolean {
  return speakable(value, node_id) !== null;
}

/** 要念的那一句（`null` = 不说：样本不足 / 不偏长 / 已经不再提）。 */
export function pickChapterNudge(
  value: ChapterLengthBoard | null,
  work_id: number | null,
  node_id: number | null,
): ChapterLengthTip | null {
  const found = speakable(value, node_id);
  if (found === null || work_id === null) return null;
  const { report, item } = found;
  return {
    work_id,
    node_id: item.node_id,
    chars: item.chars,
    median: report.median,
    // 一位小数："2.4 倍"；四舍五入在界面这一侧（核心给的是原始字数）
    times: Math.round((item.chars / report.median) * 10) / 10,
    fingerprint: item.fingerprint,
  };
}

/**
 * 「就这样写」之后：这一章以后不再提。
 *
 * 本地这一份先记上（条要立刻收、角标要立刻消失）；**真正那本账在核心**——
 * 下一次读库以核心那份为准（所以这里只改本地副本，不另立第二份记账）。
 */
export function markKept(
  value: ChapterLengthBoard | null,
  node_id: number,
): ChapterLengthBoard | null {
  if (value === null) return null;
  return {
    ...value,
    chapters: value.chapters.map((item) =>
      item.node_id === node_id ? { ...item, dismissed: true } : item,
    ),
  };
}

/** 条上那一句（键名只在这里出现一次；组件不必知道有哪几种说法）。 */
export function chapterTipText(tip: ChapterLengthTip): string {
  return t("break.chapter", { chars: tip.chars, median: tip.median, times: tip.times });
}

/** 章长提醒要的那两件事（真件在 `api/chapter-length.ts`；单测喂假的）。 */
export interface ChapterLengthTransport {
  /** 读一眼这本书的分布（只读） */
  report: (work_id: number) => Promise<ChapterLengthBoard>;
  /** 记下"这一章以后不再提"（走既有的忽略记账，记号由核心给） */
  keep: (work_id: number, fingerprint: string) => Promise<void>;
}

export interface ChapterLengthWatch {
  /**
   * 停下点那一刻：读一眼这本书的分布，判"刚离开的那一章"（`null` = 不说）。
   *
   * ⚠️ **只有停下点才调它**——接线的三个停下点各自调一次；写作途中没有任何入口。
   */
  atStop: (work_id: number | null, node_id: number | null) => Promise<ChapterLengthTip | null>;
  /**
   * 换书时把分布读进来（**不发声**）：目录角标要这份事实，
   * 但"刚打开这本书"不是一个"要停下来跟他说一句"的时刻。
   */
  refresh: (work_id: number | null) => Promise<void>;
  /** 「就这样写」：这一章以后不再提（写进核心那本账 + 本地这一份跟着改） */
  keep: (work_id: number, node_id: number, fingerprint: string) => Promise<void>;
}

export function useChapterLengthWatch(transport: ChapterLengthTransport): ChapterLengthWatch {
  /** 每一次读发一个号：异步回来的结果只认**最新那一次**。 */
  let token = 0;

  /**
   * 读一眼分布并把它摆上（`null` = 这一次不算数：读不出来，或期间又发起了新的一次）。
   *
   * 为什么要号：两次停下点挨得很近时（比如切章紧接着关窗），后发的那次才算数——
   * 拿前一次的数字去开口，念出来的就是**上一章**的事实。
   */
  async function load(work_id: number): Promise<ChapterLengthBoard | null> {
    const mine = ++token;
    let next: ChapterLengthBoard;
    try {
      next = await transport.report(work_id);
    } catch {
      return null; // 读不出来一声不响（宁可少说一句，也不拿猜的数字开口）
    }
    if (mine !== token) return null;
    board.value = next;
    return next;
  }

  return {
    async atStop(work_id, node_id) {
      if (work_id === null || node_id === null) return null;
      const next = await load(work_id);
      return next === null ? null : pickChapterNudge(next, work_id, node_id);
    },

    async refresh(work_id) {
      if (work_id === null) return;
      await load(work_id);
    },

    async keep(work_id, node_id, fingerprint) {
      try {
        await transport.keep(work_id, fingerprint);
      } catch {
        // 没记上就不假装记上了：下一回读库还是"没说过"，这一章会再被提一次（那是实话）
        return;
      }
      board.value = markKept(board.value, node_id);
    },
  };
}
