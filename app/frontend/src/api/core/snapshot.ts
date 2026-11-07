// 版本快照与回收站。

import { call, COMMANDS } from "./transport";
import type { RestorePreview } from "./backup";
import type { SourceBreakdown } from "../source";

/** 一条章节版本快照的摘要（**正文不在这里**，要比对时才拉）。 */
export interface SnapshotSummary {
  id: number;
  char_count: number;
  /** close / stuck / desync / keep / before_restore——界面查字典渲染 */
  reason: string;
  /** 作者亲手留的版本（不参与滚动删除） */
  pinned: boolean;
  created_at: number;
}

/** 差异里的一行。 */
export interface SnapshotDiffLine {
  /** same / added / removed / skipped */
  kind: string;
  text: string;
  /** 在快照那一版里的行号；新增行为 null */
  old_line: number | null;
  /** 在当前稿里的行号；删除行为 null */
  new_line: number | null;
  /** 折叠掉的相同行数（只有 skipped 非 0） */
  hidden: number;
}

/** 某条快照与**当前正文**的比对结果。 */
export interface SnapshotDiff {
  lines: SnapshotDiffLine[];
  added: number;
  removed: number;
  /** 块太大，没有逐行对齐（界面要说清"只给到这一层"） */
  truncated: boolean;
  current_char_count: number;
}

/** 一次回滚的回执：回滚后的正文（界面据此换掉编辑器内容并重建落盘基准）。 */
export interface SnapshotRestoreAck {
  node_id: number;
  body: string;
  char_count: number;
  chars_no_punct: number;
  word_count: number;
  fingerprint: string;
  /** 回滚之后的来源分布（没动到的字保留原来源） */
  sources: SourceBreakdown;
}

/** 回收站里的一项。 */
export interface TrashEntry {
  /** 「work」= 整本书；「node」= 书里被删的一段 */
  kind: string;
  id: number;
  title: string;
  work_id: number;
  work_title: string;
  deleted_at: number;
  /** 跟着一起进来 / 会一起回去的节点数（含它自己） */
  nodes: number;
}

/** 这一章有哪些版本（**新的在前**，只有摘要，没有正文）。 */
export const snapshotList = (node_id: number) =>
  call<SnapshotSummary[]>(COMMANDS.snapshotList, { node_id });

/** 某条快照与**当前正文**差在哪（行级统一差异，结构化给界面画）。 */
export const snapshotDiff = (node_id: number, snapshot_id: number) =>
  call<SnapshotDiff>(COMMANDS.snapshotDiff, { node_id, snapshot_id });

/** 手动留一版；内容与最新一份相同就返回 null（不堆重复版本）。 */
export const snapshotKeep = (node_id: number) =>
  call<number | null>(COMMANDS.snapshotKeep, { node_id });

/** 删掉一条快照（**正文一个字都不动**）。 */
export const snapshotDrop = (snapshot_id: number) =>
  call<void>(COMMANDS.snapshotDrop, { snapshot_id });

/** 回滚到某一条快照——核心会**先替当前这一版留底**，再改正文。 */
export const snapshotRestore = (snapshot_id: number) =>
  call<SnapshotRestoreAck>(COMMANDS.snapshotRestore, { snapshot_id });

/** 回收站：整本书在前，书里被删的段在后。 */
export const listTrash = () => call<TrashEntry[]>(COMMANDS.listTrash);

/** 从回收站恢复一本书。 */
export const restoreWork = (work_id: number) =>
  call<number>(COMMANDS.restoreWork, { work_id });

/** 恢复前先看一眼：会不会与同级某章重名（有冲突时界面要让作者拿主意）。 */
export const restorePreview = (node_id: number) =>
  call<RestorePreview>(COMMANDS.restorePreview, { node_id });

/**
 * 从回收站恢复一段（整棵子树 + 还在回收站里的父链），返回恢复的节点数。
 *
 * `title` 是作者给的新名字（留空 = 照原样恢复）；**名字永远由他给，系统不替他起**。
 */
export const restoreNode = (node_id: number, title: string | null) =>
  call<number>(COMMANDS.restoreNode, { node_id, title });

/** 彻底删除一段（**不可恢复**），返回删掉的节点数。 */
export const purgeNode = (node_id: number) => call<number>(COMMANDS.purgeNode, { node_id });

/** 彻底删除一本书（**不可恢复**），返回删掉的节点数。 */
export const purgeWork = (work_id: number) => call<number>(COMMANDS.purgeWork, { work_id });

/** 清空回收站，返回清掉的项数。 */
export const emptyTrash = () => call<number>(COMMANDS.emptyTrash);
