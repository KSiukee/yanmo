// 编辑器文档 ↔ 库里的正文。
//
// **库里只存纯文本**，段落之间空行分隔（Markdown 的最小公共子集）：
// 这样全文检索不会被标签污染、字数统计口径干净、将来写 .md 镜像也是直通的。
// 富文本标记（粗体 / 标题 / 批注）等中文排版任务定案后，再在这里扩展序列化——
// 序列化只有这一个出口，别在组件里各写一份。
import type { Editor } from "@tiptap/core";

/** 段落分隔符：空行。 */
const PARAGRAPH_SEPARATOR = "\n\n";

/** 编辑器文档 → 纯文本（段落之间空行）。 */
export function docToText(editor: Editor): string {
  const size = editor.state.doc.content.size;
  return editor.state.doc.textBetween(0, size, PARAGRAPH_SEPARATOR).trim();
}

/** 纯文本 → 段落列表（去掉空段与首尾空白）。 */
export function paragraphsOf(text: string): string[] {
  return text
    .split(/\n{2,}/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

/** 纯文本 → 编辑器可接受的 HTML（每个段落一个 `<p>`）。 */
export function textToHtml(text: string): string {
  return paragraphsOf(text)
    .map((line) => `<p>${escapeHtml(line)}</p>`)
    .join("");
}

function escapeHtml(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
