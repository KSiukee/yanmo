// 排版清理（只报告，勾了才动）。

import { call, COMMANDS } from "./transport";

/** 一条排版规则：稳定代码 + 风险档（safe 默认勾 / careful 自己勾 / style 默认不动）。 */
export interface TypesetRule {
  code: string;
  tier: string;
}

/** 扫描出来的一处建议改动——**只是建议**，勾了才改。 */
export interface TypesetChange {
  /** 哪条规则提出来的（名字与说明在字典的 typeset.rule.* 里） */
  rule: string;
  /** 改前那一小段（原文里的样子；插入类改动这里是空串） */
  before: string;
  /** 改后（删除类改动这里是空串） */
  after: string;
  /** 落在第几段（从 1 起） */
  paragraph: number;
  context_before: string;
  context_after: string;
}

/** 一处**只报告、不给改法**的提醒（例如引号缺一半：补哪边只有作者知道）。 */
export interface TypesetNotice {
  /** 哪个检查提出来的（名字在字典的 typeset.notice.* 里） */
  rule: string;
  /** 哪一对符号（quote_double / corner / paren / title…），名字在 typeset.mark.* 里 */
  mark: string;
  /** unclosed（开着的没关）/ unopened（关着的没有开），句子在 typeset.side.* 里 */
  side: string;
  /** 落在第几段（从 1 起） */
  paragraph: number;
  context_before: string;
  context_after: string;
}

/** 一次扫描的结果：能改的与只能提醒的，分开放。 */
export interface TypesetReport {
  changes: TypesetChange[];
  notices: TypesetNotice[];
}

/** 扫描选项（现在是引号风格一种）。 */
export interface TypesetOptions {
  quote_style: string;
}

/** 排版规则清单（代码 + 风险档）——界面不自己抄一份"有哪些规则"。 */
export const typesetRules = () => call<TypesetRule[]>(COMMANDS.typesetRules);

/** 扫一遍排版（dry-run）：**只报告会怎么改，正文一个字不动**。 */
export const typesetScan = (text: string, options: TypesetOptions) =>
  call<TypesetReport>(COMMANDS.typesetScan, { text, options });

/** 应用勾中的那几处，返回改好的正文；序号对不上会整批拒绝（稿子又改过了）。 */
export const typesetApply = (text: string, options: TypesetOptions, accepted: number[]) =>
  call<string>(COMMANDS.typesetApply, { text, options, accepted });
