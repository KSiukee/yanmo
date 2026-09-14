// 正文编辑器的扩展清单：**只允许「段落 + 文字」**。
//
// 为什么要把富文本全关掉：研墨落盘的是**纯文本**（`docToText` 只取文字），
// 而 StarterKit 默认带 6 个标记（粗体 / 斜体 / 删除线 / 行内代码 / 链接 / 下划线）与一批块级节点
// （引用 / 列表 / 标题 / 分割线）。任何一样被按出来，都会写进编辑器文档、**落盘后却消失**——
// 作者看到的现象是"改了但看不出、字数也不动"，属于最难查的一类毛病。
//
// 段内换行（Shift+Enter）同样关掉：它是 `hardBreak` 节点，取文本时不带它，
// 于是"换行后的字"会被静默接到上一行（实测过：`…然后换行：换行后的字。`）。
// 宁可这个键什么都不做，也不要静默吃掉作者的一次回车。
//
// 保留：段落 / 文字 / **撤销重做（必须有）** / 结尾自动补空段 / 光标与拖拽指示。
// 这份清单有测试盯着（它会去问真正的 schema，而不是核对这份配置本身）。

import StarterKit from "@tiptap/starter-kit";

/**
 * 关掉的"看不见的格式"：**标记 6 个 + 块级节点 8 个**。
 *
 * 加新项要连测试一起改：测试会拿完整版 StarterKit 的 schema 与这边对照，
 * 漏关一个（或 StarterKit 将来新增一个）都会红。
 */
export const PLAIN_TEXT_KIT = {
  // 标记：落盘后一个字都不剩，纯属"隐形改动"
  bold: false,
  italic: false,
  strike: false,
  code: false,
  link: false,
  underline: false,
  // 块级节点：同样是"看不见的结构"（纯文本里表达不出来）
  blockquote: false,
  bulletList: false,
  orderedList: false,
  listItem: false,
  codeBlock: false,
  heading: false,
  horizontalRule: false,
  // 段内换行：取文本时会丢，见文件头说明
  hardBreak: false,
} as const;

/** 正文编辑器用的扩展：只剩段落与文字。 */
export function plainTextExtensions() {
  return [StarterKit.configure(PLAIN_TEXT_KIT)];
}
