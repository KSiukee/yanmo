// 镜像状态那一行的**取舍**（纯逻辑，单独放便于盯住）。
//
// 为什么单独一份：这一行要在四种"事实"之间选一种说法——关着 / 还没对过 / 对上了多少 /
// 有冲突。选错的代价不是崩，而是**作者对"磁盘上到底有没有那份 .md"形成错误印象**，
// 而那正是这个功能存在的全部理由。所以让机器盯着，别散在模板里靠肉眼。

import type { MirrorIssue, MirrorStatus } from "../api/mirror";

/** 状态行该说什么（渲染层再把它翻成文案）。 */
export type MirrorLine =
  | { kind: "off" }
  | { kind: "never" }
  | { kind: "synced"; files: number; at: number };

/**
 * 选一行说法。**没有报告时按"还没对过"**——不猜一个"已对上 0 份"的假绿。
 */
export function mirrorLine(status: MirrorStatus | null): MirrorLine {
  if (status === null || !status.enabled) {
    return status === null ? { kind: "never" } : { kind: "off" };
  }
  if (status.last_sync_at <= 0) {
    return { kind: "never" };
  }
  return { kind: "synced", files: status.files, at: status.last_sync_at };
}

/** 时刻写成 `时:分:秒`（作者自己机器上的本地时间，不做时区换算）。 */
export function clockText(millis: number): string {
  const date = new Date(millis);
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

/** 一条待定夺的事在列表里的稳定键（同一个路径先后换过类型也算同一条）。 */
export function issueKey(issue: MirrorIssue): string {
  return `${issue.kind}:${issue.relative_path}`;
}

/**
 * 这一条能给作者哪几条路。
 *
 * **没账的文件一条都不给**：研墨不知道它是哪一章，也不该去动一份自己没有账的东西——
 * 只把路径报出来，让作者自己去看看（多半是他把文件挪了名字）。
 */
export function issueActions(issue: MirrorIssue): { adopt: boolean; overwrite: boolean } {
  const ours = issue.kind !== "untracked" && issue.node_id > 0;
  return { adopt: ours, overwrite: ours };
}
