// 作者信息：投稿时随稿子走的那点身份（笔名 / 真名 / 联系方式 / 文学简历）。
//
// 这一层只做"取值 ↔ 参数"的转换；稀疏合并、坏记录当没设过、留痕不抄取值全在核心。
//
// 两条口径：
// - **跟人、不跟书**：填一次，所有作品共用（写作软件里的作者信息本来就该这样）；
// - **敏感项默认不写进稿件**：身份证号与银行账号存下来是为了"某家刊物要时不用再翻"，
//   真正写不写进文件由核心那个开关定。

import { call, COMMANDS } from "./core";

/** 作者信息（读出来给人用的那一份；没设过的栏是空串）。 */
export interface Author {
  pen_name: string;
  real_name: string;
  phone: string;
  email: string;
  address: string;
  postcode: string;
  bio: string;
  /** 敏感：身份证号（**默认不写进稿件**） */
  id_number: string;
  /** 敏感：银行账号与开户行（**默认不写进稿件**） */
  bank: string;
  /** 投稿时附不附作者简介（默认不附） */
  attach: boolean;
  /** 附的时候带不带敏感两项（默认不带） */
  with_sensitive: boolean;
}

/** 读作者信息（没设过就是一份空的）。 */
export const authorRead = () => call<Author>(COMMANDS.authorRead);

/** 改作者信息（只写传进来的那几栏；写完回读）。 */
export const authorWrite = (patch: Partial<Author>) =>
  call<Author>(COMMANDS.authorWrite, { patch });

/** 清空（回到"没设过"）。 */
export const authorReset = () => call<Author>(COMMANDS.authorReset);
