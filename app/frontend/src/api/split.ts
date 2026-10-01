// 拆章：在光标所在那一段之前把一章切成两章（**只由作者触发，绝不自动拆**）。
//
// 单独一个文件的原因与 `api/chapter-length.ts` 同一条：命令类型与薄封装跟着**功能**走，
// 不必把它们塞进 `api/core.ts`（那一份是"核心网关"的公共面，谁都能往里加一行，于是它越滚越长）。
//
// 两个口径（与核心一致）：
// - **切点用段落下标**（0 起）：正文在库里存的就是"纯文本 + 空行分段"，按段落切，
//   **段落完整性是结构上保证的**——不存在"按字数硬切把一段拦腰截断"；
// - **调用前必须先落盘**：核心读的是库里那一份，手上没存完的字不在里面。

import { call, COMMANDS, type EditorSnapshot } from "./core";

/** 一次拆章的回执：新章那一屏 + 撤销要的那几样。 */
export interface SplitAck {
  /** 新章那一屏（界面照"新建一章"那条路切过去） */
  new_chapter: EditorSnapshot;
  receipt: SplitReceipt;
}

/** 拆章的回执本体（撤销照原样交回来，界面不自己拼这几个数）。 */
export interface SplitReceipt {
  old_node_id: number;
  new_node_id: number;
  /** 拆之前留的那一份底（作者也能去「版本」里手动回滚） */
  snapshot_id: number;
  /** 留在原章的字数 */
  kept_chars: number;
  /** 分出去的字数 */
  moved_chars: number;
  /** 新章的标题（名字留空时，这是核心按序号取的那个） */
  new_title: string;
}

/**
 * 在**第 `at` 段之前**把这一章切成两章。
 *
 * `at` 段落下标由界面按光标算好（口径：光标所在那一段的开头，见 `editor/split.ts`）。
 */
export const chapterSplit = (node_id: number, at: number, title: string) =>
  call<SplitAck>(COMMANDS.chapterSplit, { node_id, at, title });

/** 撤销一次拆章：两章合回原章、新章进回收站，回的是**原章**那一屏。 */
export const chapterSplitUndo = (receipt: SplitReceipt) =>
  call<EditorSnapshot>(COMMANDS.chapterSplitUndo, {
    old_node_id: receipt.old_node_id,
    new_node_id: receipt.new_node_id,
    snapshot_id: receipt.snapshot_id,
  });
