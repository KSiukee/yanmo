// 故事总纲：**整本书讲什么**（立意 / 主线 / 卖点）。
//
// 与作品简介（`setWorkSummary`）分开两格：简介是给编辑看的（投稿 docx 里摆在书名下），
// 总纲是给自己看的（投稿包里在大纲前面单独一节）。两处都存作者原话，核心不做修剪。
//
// 这一层只做"取值 ↔ 参数"的转换。

import { call, COMMANDS } from "./core";

/** 读一本书的故事总纲（没写过就是空串）。 */
export const workStoryline = (work_id: number) =>
  call<string>(COMMANDS.workStoryline, { work_id });

/** 写故事总纲，回**库里真有的那一份**（界面显示的永远是库里的值）。 */
export const setWorkStoryline = (work_id: number, text: string) =>
  call<string>(COMMANDS.workSetStoryline, { work_id, text });
