// 拆章的**那一点纯逻辑**：从编辑器里的光标算出"第几段之前切开"。
//
// 判据只有一条（作者拍板的口径）：**按光标所在那一段的开头切**——光标的段落及其后面
// 全部进新章。好处是可预期：把光标丢在目标段落的哪个位置，结果都一样。
//
// 为什么算出来的是**段落下标**而不是"第几个字"：正文在库里存的是纯文本、空行分段
// （口径在核心 `text::paragraph_spans`）。按段落切，**段落完整性就是结构上保证的**，
// 不存在"按字数硬切把一段拦腰截断"这种事——这正是这条任务要的。
//
// 本文件不认识 Vue、不认识 Tauri：单测喂一个假的文档结构进去就行。

/** 编辑器文档里一个节点的一小片（只用到判断"是不是段落、多长、空不空"这三样）。 */
export interface DocNode {
  type: { name: string };
  /** 这一个节点在文档里占几个位置（开闭标记各算一个，与 ProseMirror 一致） */
  nodeSize: number;
  textContent: string;
}

/** 编辑器文档（`editor.state.doc` 结构上就是这个形状）。 */
export interface DocLike {
  forEach: (visit: (node: DocNode, offset: number) => void) => void;
}

/** 切点：界面上要显示与交给核心的那几个数。 */
export interface SplitPoint {
  /** 第几段之前切开（0 起）——**交给核心的就是它** */
  at: number;
  /** 这一章一共几段（**非空**段：与库里那份正文同一口径） */
  total: number;
  /** 留在原章的段数 */
  kept: number;
  /** 分出去的段数 */
  moved: number;
}

/**
 * 光标落在这儿时，该从第几段之前切开。
 *
 * 返回 `null` = **这一刀切不了**（只有一段、或者切在开头/末尾）：界面据此把按钮置灰，
 * 不让作者点进去再被拒一次。
 *
 * 判据：数**结束位置在光标之前（含正好落在光标上）的非空段落**有几个——那就是 `at`。
 * 于是"光标在某段中间"与"光标在它开头"给出同一个答案（都在那一段之前切），
 * 而"光标正好在某段末尾"会把它算进前半（切点在下一段之前）。
 */
export function splitPointOf(doc: DocLike, from: number): SplitPoint | null {
  const paragraphs: { end: number; empty: boolean }[] = [];
  doc.forEach((node, offset) => {
    if (node.type.name !== "paragraph") return; // 只数段落
    paragraphs.push({
      // 段落占 [offset, offset + nodeSize)；光标停在段内，最大到 nodeSize - 1
      end: offset + node.nodeSize - 1,
      empty: node.textContent.trim() === "",
    });
  });
  const filled = paragraphs.filter((paragraph) => !paragraph.empty);
  const total = filled.length;
  if (total < 2) return null; // 只有一段（或一段都没有）：没得拆
  const at = filled.filter((paragraph) => paragraph.end <= from).length;
  if (at <= 0 || at >= total) return null; // 切在开头 / 末尾：切出来一边是空的
  return { at, total, kept: at, moved: total - at };
}

/** 这一段是不是能拆（界面拿它决定按钮置不置灰）。 */
export function canSplit(editor: { state: { doc: DocLike; selection: { from: number } } } | null): boolean {
  if (!editor) return false;
  return splitPointOf(editor.state.doc, editor.state.selection.from) !== null;
}
