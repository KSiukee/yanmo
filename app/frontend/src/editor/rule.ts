// 稿纸横线（线型：实线 / 虚线 / 不显示）——**纯显示层**：
// 只在正文底下垫一层横线，不进导出、不动正文一个字节，也不改正文的行数（线跟着行距走）。
//
// 两件事在这里合成一组 CSS 变量：
//   ① 作者选的是哪一档（核心 `appearance.editor_rule_style` 的稳定码）；
//   ② 一格横线多高——**与正文一行一样高**（字号 × 行距），线才落在每一行的底下。
// 线怎么画、用什么颜色在样式表里（`--ym-rule-band` 见 components/editor-pane.css，
// 颜色 `--ym-rule` 见色板 style.css 的两套）。
//
// 为什么档位与换算只写在这一处：组件里写死 px 就会跟排版那一栏走偏——同一份偏好算出来的
// 两个数（行高与线距）必须永远一致，否则线会慢慢从字底下漂开，而那是最难查的一类观感毛病。

import type { Appearance } from "../api/core.ts";
import { asTypography } from "./typography.ts";

/** 线型（稳定码与核心 `RuleStyle` 一致；进库存的就是这三个）。 */
export type RuleStyle = "none" | "solid" | "dashed";

/** 三档（面板上的顺序；穷举测试拿它对表）。 */
export const RULE_STYLES: readonly RuleStyle[] = ["solid", "dashed", "none"];

/** 默认那一档：**不显示**（与一直以来的样子一致；核心那边也是这个默认）。 */
export const DEFAULT_RULE_STYLE: RuleStyle = "none";

/** 库里那份（可能是 null / 缺项 / 不认识的码）→ 能用的一档。 */
export function asRuleStyle(values: Partial<Appearance> | null | undefined): RuleStyle {
  const raw = values?.editor_rule_style;
  return raw === "solid" || raw === "dashed" ? raw : DEFAULT_RULE_STYLE;
}

/**
 * 一格横线有多高（px）：**与正文一行一样高**（字号 × 行距），线才落在每一行的底下。
 *
 * 保留两位小数：行距是百分比，乘出来常常是 32.3 这种带小数的值——四舍五入回整数就会
 * 一行一行地积起偏差（写到几十行后，线已经离开字底了）。
 */
export function ruleStep(values: Partial<Appearance> | null | undefined): number {
  const { size, line } = asTypography(values);
  return Math.round(size * line) / 100;
}

/** 绑到编辑区上的那一组 CSS 变量（`--ym-rule-*`）。 */
export function ruleStyle(values: Partial<Appearance> | null | undefined): Record<string, string> {
  return { "--ym-rule-step": `${ruleStep(values)}px` };
}
