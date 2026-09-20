// 稿件台：看板读数、记一次投稿、改状态、改 / 删投稿记录。
//
// 这一层只做"取值 ↔ 参数"的转换；状态机的合法性、冷冻期与一稿多投判据、留痕全在核心。
//
// 三个口径（与核心一致）：
// - **不强制顺序**：草稿 / 完稿 / 待投 之间怎么挪都行，没有"下一步"门禁；
// - **进「已投」不是拖拽，而是记一次投稿**（要有投向与时刻）——所以它不在 `nextStatuses` 里；
// - **一稿多投是软拦**：`manuscriptVerdict` 只说明后果，作者确认后 `manuscriptSubmit` 照样记。

import { call, COMMANDS } from "./core";

/** 稿件的七态（与核心 `ManuscriptState` 的稳定码一致）。 */
export type ManuscriptStatus =
  | "draft"
  | "finished"
  | "ready"
  | "submitted"
  | "accepted"
  | "rejected"
  | "published";

/** 看板的列顺序与"拖拽能去哪几个态"在 `editor/manuscript.ts`（纯规则，能脱离界面单测）。 */

/** 一次投稿的结果（与核心 `SubmissionResult` 的稳定码一致）。 */
export type SubmissionResultCode = "waiting" | "accepted" | "rejected" | "withdrawn";

/** 一次投稿。 */
export interface Submission {
  id: number;
  venue: string;
  submitted_at: number;
  result: SubmissionResultCode;
  /** 冷冻期截止；null = 没定期限（判据里算"还在等"） */
  cooldown_until: number | null;
  notes: string;
}

/** 看板的一张卡片。 */
export interface ManuscriptRow {
  id: number;
  /** novel / article / collection */
  kind: string;
  title: string;
  status: ManuscriptStatus;
  word_count: number;
  char_count: number;
  /** 逐字（不含标点）——与书架同一份口径，按作品的字数口径挑一格显示 */
  chars_no_punct: number;
  /** 目标字数（null = 没设过）——「未完成散篇」比的就是它 */
  target_words: number | null;
  /** 投过几家（软删的不算） */
  submissions: number;
  /** 还在等回音的那条；没有就是三个 null */
  active_venue: string | null;
  active_submitted_at: number | null;
  active_cooldown_until: number | null;
  /** 最近投给谁（含已出结果的） */
  last_venue: string | null;
  updated_at: number;
}

/** 一篇稿子的详情。 */
export interface ManuscriptDetail {
  work_id: number;
  status: ManuscriptStatus;
  /** 新的在前 */
  submissions: Submission[];
}

/** 判词码：放行 / 还在等别家回音 / 这家冷冻期未满 / 这家投过。 */
export type SubmitVerdictCode = "clear" | "awaiting_reply" | "venue_cooldown" | "already_tried";

/** 记一次投稿之前的那句判词（不是门禁：作者确认后照样能投）。 */
export interface SubmitVerdict {
  code: SubmitVerdictCode;
  /** 判词针对的那一家（clear 时是 null） */
  venue: string | null;
  /** 相关时间点：还在等=那次投稿的时刻；这家投过=那一次的时刻 */
  at: number | null;
  /** 冷冻期到哪天 */
  cooldown_until: number | null;
}

/** 记一次投稿的回执。 */
export interface ManuscriptSubmitResult {
  from: ManuscriptStatus;
  verdict: SubmitVerdict;
  detail: ManuscriptDetail;
}

/**
 * 拖拽能直接走到哪些态（与核心的迁移表同源；**核心仍会拒非法边**）。
 *
 * 不含 `submitted`：进「已投」要有投向与时刻，那是"记一次投稿"（见 [`manuscriptSubmit`]）。
 */
export function nextStatuses(status: ManuscriptStatus): ManuscriptStatus[] {
  switch (status) {
    case "draft":
      return ["finished", "ready"];
    case "finished":
      return ["draft", "ready"];
    case "ready":
      return ["draft", "finished", "accepted", "rejected"];
    case "submitted":
      return ["accepted", "rejected", "ready"];
    case "accepted":
      return ["published"];
    case "rejected":
      return ["draft", "finished", "ready"];
    case "published":
      return [];
  }
}

/** 看板一屏：卡片 + 界面填表要用的默认值。 */
export interface ManuscriptBoard {
  /** 没写期限时预填几天（核心给的默认冷冻期；**只是预填**） */
  default_cooldown_days: number;
  rows: ManuscriptRow[];
}

/** 看板：每篇没进回收站的稿子一行。 */
export const manuscriptBoard = () => call<ManuscriptBoard>(COMMANDS.manuscriptBoard);

/** 一篇稿子的详情（状态 + 全部投稿记录）。 */
export const manuscriptDetail = (work_id: number) =>
  call<ManuscriptDetail>(COMMANDS.manuscriptDetail, { work_id });

/** 记之前先问一句（只读，只说明后果）。 */
export const manuscriptVerdict = (work_id: number, venue: string) =>
  call<SubmitVerdict>(COMMANDS.manuscriptVerdict, { work_id, venue });

/** 记一次投稿：写历史 + 状态推进到「已投」（核心同一个事务里做）。 */
export const manuscriptSubmit = (
  work_id: number,
  venue: string,
  submitted_at: number,
  cooldown_until: number | null,
  notes = "",
) =>
  call<ManuscriptSubmitResult>(COMMANDS.manuscriptSubmit, {
    work_id,
    venue,
    submitted_at,
    cooldown_until,
    notes,
  });

/** 改状态（看板拖拽走这条）；`to` 传七态稳定码。 */
export const manuscriptMove = (work_id: number, to: ManuscriptStatus) =>
  call<ManuscriptDetail>(COMMANDS.manuscriptMove, { work_id, to });

/** 改一条投稿记录（记错目标 / 补期限 / 添备注）。 */
export const manuscriptUpdateSubmission = (
  submission_id: number,
  venue: string,
  submitted_at: number,
  cooldown_until: number | null,
  notes = "",
) =>
  call<ManuscriptDetail>(COMMANDS.manuscriptUpdateSubmission, {
    submission_id,
    venue,
    submitted_at,
    cooldown_until,
    notes,
  });

/** 软删一条投稿记录（还在等回音的那条核心会拒——先撤稿）。 */
export const manuscriptDeleteSubmission = (submission_id: number) =>
  call<ManuscriptDetail>(COMMANDS.manuscriptDeleteSubmission, { submission_id });

/** 设定 / 清除目标字数（null 或 ≤0 = 清掉）；回读库里那份。 */
export const setWorkTargetWords = (work_id: number, words: number | null) =>
  call<number | null>(COMMANDS.setWorkTargetWords, { work_id, words });
