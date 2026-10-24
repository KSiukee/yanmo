// 正文来源的**界面契约**：这一章的字从哪来、这一笔落盘是什么来源。
//
// 与核心的 `store::SourceBreakdown` / `model::WriteOrigin` 一一对应：
//
// - `source` 是**稳定码**（typed / pasted / module / voice / system / mixed / unknown）；
//   界面按 `editor.source.<码>` 查字典，**不在这里写中文**（文案只有 locales 一处）。
// - 三档字数与正文的三口径同一套规则，**各档加起来永远等于总数**
//   （核心用一遍扫描按来源分桶，见 `text::tally_by_source`）。
// - `SaveOrigin` 是**壳报给核心的观测**：只有壳看得见键入 / 粘贴。
//   没报就是"未记录"——**绝不默认成打字**（那是往留痕里写假数据）。

/** 来源账里的一档：某个来源贡献了多少字（三个口径）。 */
export interface SourceShare {
  source: string;
  char_count: number;
  chars_no_punct: number;
  word_count: number;
}

/** 一章的来源分布（只含有字的档，顺序由核心定）。 */
export interface SourceBreakdown {
  shares: SourceShare[];
}

/** 一次落盘的来源（壳的观测；`kind` 是稳定码）。 */
export interface SaveOrigin {
  kind: string;
  /** 这次落盘期间剪贴板里的 plain text 字数（只作证据，不参与归属计算） */
  clipboard_chars?: number;
  /** 模块交付才有：模块 id 与模型名（缺了核心拒收） */
  module_id?: string;
  model?: string;
}
