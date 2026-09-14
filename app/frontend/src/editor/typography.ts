// 正文排版的**档位与换算**：只在这一处（组件里不许写死 px / 倍数）。
//
// 三个值都存在外观偏好里（`Appearance.editor_*`），存的是**整数**：
//   字号 px、行距百分比（190 = 1.9 倍）、字距百分比（5 = 0.05em）。
// 这里把它们翻成 CSS 变量，绑到编辑区上；**只影响观感**——不进导出、不动正文一个字节
// （核心那边有验收钉着）。
//
// 为什么默认值写在这儿而不是核心：核心只管"作者改过什么"（没改过就是 `None`），
// "17px / 1.9 倍看起来舒服"是界面的事。档位表放这里，设置面板与编辑区读的是同一份。

import type { Appearance } from "../api/core";

/** 落定后的三个值（没设过的项已经被默认值顶上）。 */
export interface Typography {
  size: number;
  /** 行距百分比：190 = 1.9 倍 */
  line: number;
  /** 字距：em 的百分之几（5 = 0.05em） */
  spacing: number;
}

/** 一次都不用改的默认档：正文 17px、行距 1.9、不加字距（与老界面一致）。 */
export const TYPOGRAPHY_DEFAULT: Typography = { size: 17, line: 190, spacing: 0 };

/** 一个旋钮：范围、步进、怎么显示、绑哪个 CSS 变量。 */
export interface TypographyKnob {
  /** 偏好字段名（发回核心时用的就是它） */
  field: TypographyField;
  /** 绑到编辑区上的 CSS 变量名 */
  cssVar: string;
  /** 面板上那一行的文案键 */
  labelKey: string;
  min: number;
  max: number;
  step: number;
  /** 给人看的那一份（`17 px` / `1.9` / `5%`） */
  format: (value: number) => string;
  /** 给 CSS 用的那一份（`17px` / `190%` / `0.05em`） */
  css: (value: number) => string;
}

export type TypographyField = "editor_font_size" | "editor_line_height" | "editor_letter_spacing";

/** 三个旋钮（顺序 = 面板上的顺序）。范围与核心一致：核心夹一次防坏数据，这里防手滑。 */
export const TYPOGRAPHY_KNOBS: readonly TypographyKnob[] = [
  {
    field: "editor_font_size",
    cssVar: "--ym-body-size",
    labelKey: "settings.typography_size",
    min: 12,
    max: 30,
    step: 1,
    format: (value) => `${value} px`,
    css: (value) => `${value}px`,
  },
  {
    field: "editor_line_height",
    cssVar: "--ym-body-line",
    labelKey: "settings.typography_line",
    min: 110,
    max: 260,
    step: 5,
    format: (value) => String(value / 100),
    css: (value) => `${value}%`,
  },
  {
    field: "editor_letter_spacing",
    cssVar: "--ym-body-spacing",
    labelKey: "settings.typography_spacing",
    min: 0,
    max: 20,
    step: 1,
    format: (value) => (value === 0 ? "0" : `${value}%`),
    css: (value) => `${value / 100}em`,
  },
];

function valueOf(typography: Typography, field: TypographyField): number {
  if (field === "editor_font_size") return typography.size;
  if (field === "editor_line_height") return typography.line;
  return typography.spacing;
}

/** 库里那份（可能是 null / 缺项 / 越界）→ 可直接绑到界面上的三个值。 */
export function asTypography(values: Partial<Appearance> | null | undefined): Typography {
  const pick = (field: TypographyField, fallback: number, min: number, max: number) => {
    const raw = values?.[field];
    if (typeof raw !== "number" || !Number.isFinite(raw) || raw <= 0) return fallback;
    return Math.min(max, Math.max(min, Math.round(raw)));
  };
  return {
    size: pick("editor_font_size", TYPOGRAPHY_DEFAULT.size, 12, 30),
    line: pick("editor_line_height", TYPOGRAPHY_DEFAULT.line, 110, 260),
    spacing: pick("editor_letter_spacing", TYPOGRAPHY_DEFAULT.spacing, 0, 20),
  };
}

/** 绑到编辑区上的 CSS 变量（`--ym-body-*`）；三个值永远都给全，不留空档。 */
export function typographyStyle(
  values: Partial<Appearance> | null | undefined,
): Record<string, string> {
  const typography = asTypography(values);
  const style: Record<string, string> = {};
  for (const knob of TYPOGRAPHY_KNOBS) {
    style[knob.cssVar] = knob.css(valueOf(typography, knob.field));
  }
  return style;
}
