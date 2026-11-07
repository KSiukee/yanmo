// 「这本书单独一套」的那点数据：哪些观感项可覆盖、清掉的取值、从视图抄一份覆盖。
//
// 只放纯函数，不碰 ref / 传输——编排仍在 `appearance.ts`。

import type { Appearance } from "../api/core";

/// 允许"这本书单独一套"的观感项（其余项仍只走全局：配色、护眼、用眼提醒跟人不跟书）。
///
/// 这些项在核心侧都有明确的"清掉这一层"的写法（`auto` / 0 / 负数），
/// 所以逐项恢复"跟全局"不需要另造一套命令。
export const WORK_VISUAL_FIELDS = [
  "editor_font_size",
  "editor_line_height",
  "editor_letter_spacing",
  "editor_font",
  "editor_font_scope",
  "editor_rule_style",
  "editor_bg_color",
  "editor_bg_layout",
  "editor_bg_fit",
  "editor_bg_opacity",
] as const;

export type WorkVisualField = (typeof WORK_VISUAL_FIELDS)[number];

/** 每一项"回全局"时写给核心的取值（核心那边就是"清掉这一层"）。 */
export const CLEAR_VALUE: Record<WorkVisualField, string | number> = {
  editor_font_size: 0,
  editor_line_height: 0,
  editor_letter_spacing: 0,
  editor_font: "auto",
  editor_font_scope: "auto",
  editor_rule_style: "auto",
  editor_bg_color: "auto",
  editor_bg_layout: "auto",
  editor_bg_fit: "auto",
  editor_bg_opacity: -1,
};

/** 这一层有没有设过任何可覆盖的观感项（开关状态就看它）。 */
export function hasVisualOverrides(overridden: string[] | undefined): boolean {
  if (!overridden) return false;
  return WORK_VISUAL_FIELDS.some((field) => overridden.includes(field));
}

/** 把"当前生效值"抄成一份覆盖：数值项没设过（null）不写，避免拿 0 冒充覆盖。 */
export function visualPatchFrom(view: Appearance): Record<string, string | number> {
  const patch: Record<string, string | number> = {};
  for (const field of WORK_VISUAL_FIELDS) {
    const value = view[field];
    if (value === null || value === undefined) continue;
    patch[field] = value;
  }
  return patch;
}

/** 关掉开关：把这些项一次性清回全局。 */
export function clearVisualPatch(): Record<string, string | number> {
  const patch: Record<string, string | number> = {};
  for (const field of WORK_VISUAL_FIELDS) patch[field] = CLEAR_VALUE[field];
  return patch;
}
