// 稿件台的**纯规则**：看板列序、拖拽落不落得下、未完成散篇、倒计时与日期换算。
//
// 单独一个文件是为了能**脱离界面与核心单测**（真命令由会话层注入，见 `manuscript-board.ts`）。
// 它只认类型不认命令——`api/manuscript.ts` 那一层才碰 Tauri。
//
// 两条口径与核心一致：
// - **不强制顺序**：草稿 / 完稿 / 待投 之间怎么挪都行，没有"下一步"门禁；
// - **进「已投」不是拖拽**：那个状态的意思是"投出去了、在等回音"，要有投向与时刻。

import type { ManuscriptRow, ManuscriptStatus } from "../api/manuscript";

const DAY_MS = 86_400_000;

/** 看板的列顺序（与核心 `ManuscriptState::ALL` 同源：界面不再自己排一遍）。 */
export const BOARD_COLUMNS: ManuscriptStatus[] = [
  "draft",
  "finished",
  "ready",
  "submitted",
  "accepted",
  "rejected",
  "published",
];

/**
 * 拖拽能直接走到哪些态（与核心的迁移表同源；**核心仍会拒非法边**）。
 *
 * 刻意不含 `submitted`：进「已投」要记一次投稿（见 `manuscript-board.ts` 的 `openSubmit`）；
 * `published` 是终态，出不来。这两条是"说不通"的那两种，剩下怎么挪都行。
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
    default:
      return [];
  }
}

/** 这一步落得下吗（拖拽时问它；核心仍会拒）。 */
export function canDrop(from: ManuscriptStatus, to: ManuscriptStatus): boolean {
  return nextStatuses(from).includes(to);
}

/** 状态 → 字典键（核心只给码，句子在界面字典里）。 */
export function statusKey(status: ManuscriptStatus): string {
  return `manuscript.status.${status}`;
}

/** 判词码 → 字典键。 */
export function verdictKey(code: string): string {
  return `manuscript.verdict.${code}`;
}

/** 还在"开了坑"那两个阶段（未完成散篇只在它们里面找）。 */
function openStage(status: ManuscriptStatus): boolean {
  return status === "draft" || status === "finished";
}

/**
 * 「未完成散篇」：**只有设过目标字数的**才判得了（没目标不猜——那是替作者立规矩）。
 *
 * 判据就是设计里那句：完稿目标 vs 当前字数。**不看时间**：多久没动只用来排序，
 * 不作判据——写写停停是常态，拿"多久没打开"判人等于催稿。
 */
export function isUnfinished(
  row: Pick<ManuscriptRow, "status" | "target_words" | "char_count">,
): boolean {
  return openStage(row.status) && row.target_words !== null && row.char_count < row.target_words;
}

/** 开了坑、还没定完稿目标的：单列一句提示，不算"未完成散篇"。 */
export function needsTarget(row: Pick<ManuscriptRow, "status" | "target_words">): boolean {
  return openStage(row.status) && row.target_words === null;
}

/** 还差多少字（0 = 已经够了）。 */
export function remainingWords(row: Pick<ManuscriptRow, "target_words" | "char_count">): number {
  return row.target_words === null ? 0 : Math.max(0, row.target_words - row.char_count);
}

/** 未完成散篇那一屏：**最后编辑得最久远的排前面**（最容易被忘的那种先提醒）。 */
export function unfinishedRows(rows: ManuscriptRow[]): ManuscriptRow[] {
  return rows.filter(isUnfinished).sort((a, b) => a.updated_at - b.updated_at);
}

export function untargetedRows(rows: ManuscriptRow[]): ManuscriptRow[] {
  return rows.filter(needsTarget).sort((a, b) => a.updated_at - b.updated_at);
}

/** 距某个时刻还有几天（向上取整；已经过了就是 0 或负数，界面据此说"已超期"）。 */
export function daysUntil(ms: number, now: number = Date.now()): number {
  return Math.ceil((ms - now) / DAY_MS);
}

/** 从投稿时刻推出默认冷冻期截止（填表预填用；天数由核心给，界面不写死）。 */
export function defaultCooldownUntil(submittedAt: number, days: number): number {
  return submittedAt + days * DAY_MS;
}

/** 这条投稿的冷冻期过了吗（还在等回音、期限已过 → 视为可改投）。 */
export function overdue(cooldown_until: number | null, now: number = Date.now()): boolean {
  return cooldown_until !== null && cooldown_until <= now;
}

/**
 * 投稿已超期、可以改投的那些（顶栏那个数字徽标与稿件台里那条提示都读它）。
 *
 * 只算"在等回音、而且**定过期限**、期限已经过"的：没定期限的不算超期（作者没说等多久，
 * 不替他假设）——与核心的判据同一条口径。
 */
export function overdueRows(rows: ManuscriptRow[], now: number = Date.now()): ManuscriptRow[] {
  return rows.filter((row) => overdue(row.active_cooldown_until, now));
}

/** 日期输入框（`YYYY-MM-DD`，本地日历日）→ 当天零点的毫秒；空串 / 坏值给 null。 */
export function dateInputToMs(text: string): number | null {
  const parts = /^(\d{4})-(\d{2})-(\d{2})$/.exec(text.trim());
  if (!parts) return null;
  const ms = new Date(Number(parts[1]), Number(parts[2]) - 1, Number(parts[3])).getTime();
  return Number.isFinite(ms) ? ms : null;
}

/** 毫秒 → 日期输入框的值（本地日历日）。 */
export function msToDateInput(ms: number | null): string {
  if (ms === null) return "";
  const date = new Date(ms);
  if (!Number.isFinite(date.getTime())) return "";
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/** 按状态分好列：**七列都在**（空列也要占着，看板才是看板）。 */
export function groupByStatus(rows: ManuscriptRow[]): Record<ManuscriptStatus, ManuscriptRow[]> {
  const out = {} as Record<ManuscriptStatus, ManuscriptRow[]>;
  for (const column of BOARD_COLUMNS) out[column] = [];
  for (const row of rows) out[row.status]?.push(row);
  return out;
}
