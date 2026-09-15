// 往正文里插一段文字（叩问的答案落章走这条）。
//
// 单独成文件的理由：**它是"一次编辑"**，而会话层管的是"这一章怎么开、怎么存、怎么切"。
// 插字那点分寸（落在哪儿、按段落切、没点过光标怎么办）与切章/落盘的节奏不是一回事，
// 而且会话层那个文件已经太长（登记在案的待拆文件），能不加就不往里加。
import type { Editor } from "@tiptap/core";

import { textToHtml } from "./doc";

/**
 * 把一段文字插进**当前这一章**：走编辑器 → `onUpdate` → 自动落盘那条路，
 * 于是字数、码字账本、版本快照全都照常（核心那边不需要另一条写正文的路）。
 *
 * - `at === "cursor"`：落在当前选区（作者刚才写到哪儿就接在哪儿）；
 * - `at === "end"`：新起一段接在本章末尾。
 *
 * 光标的坑在这里补：作者没在正文里点过（这一章又没有上次读到的位置）时，编辑器的选区
 * 停在文档开头——照字面"落在光标处"会把答案甩到全章最前面。所以那种情况下按**章末**处理，
 * 界面上的说法也照实写（面板里那句落点提示）。
 *
 * 返回有没有真插进去（空文字不插）。
 */
export function insertIntoChapter(
  editor: Editor,
  text: string,
  at: "cursor" | "end",
  /** 正文里眼下有没有一个"作者放下的光标"（没点过、也没上次的位置就是没有） */
  hasCursor: boolean,
): boolean {
  const html = textToHtml(text);
  if (!html) return false;
  const toEnd = at === "end" || !hasCursor;
  const chain = editor.chain().focus();
  if (toEnd) chain.setTextSelection(editor.state.doc.content.size);
  chain.insertContent(html).run();
  return true;
}
