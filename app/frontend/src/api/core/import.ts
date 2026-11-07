// 外部文本导入（粘贴 / txt·md → 碎片池）。

import { call, COMMANDS } from "./transport";

export interface ImportPreview {
  count: number;
  total_chars: number;
  samples: string[];
}

export interface ImportOutcome {
  batch_id: string;
  imported: number;
  skipped: number;
  total: number;
}

export interface ImportBatch {
  id: string;
  label: string;
  source: string;
  kind: string;
  count: number;
  created_at: number;
  undone: boolean;
}

export interface TextFile {
  name: string;
  text: string;
}

/** 选一个 txt / md（路径在壳里；界面只拿到名字与内容）。 */
export const pickTextFile = (title: string, filter_label: string) =>
  call<TextFile | null>(COMMANDS.pickTextFile, { title, filter_label });

/** 分批预览：会切成几条、前几条长什么样（不落库）。 */
export const fragmentImportPreview = (text: string, mode: string) =>
  call<ImportPreview>(COMMANDS.fragmentImportPreview, { text, mode });

/** 真正导入：带来源（pasted）、按批可撤。 */
export const fragmentImport = (
  work_id: number,
  kind: string,
  label: string,
  text: string,
  mode: string,
) => call<ImportOutcome>(COMMANDS.fragmentImport, { work_id, kind, label, text, mode });

/** 撤销一整批导入（只软删这一批碎片，不碰正文）。 */
export const fragmentImportUndo = (batch_id: string) =>
  call<number>(COMMANDS.fragmentImportUndo, { batch_id });

/** 这本书最近的导入批次。 */
export const fragmentImportBatches = (work_id: number) =>
  call<ImportBatch[]>(COMMANDS.fragmentImportBatches, { work_id });
