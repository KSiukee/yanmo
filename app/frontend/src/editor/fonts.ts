// 正文字体：**五档预设 + 系统里装的某一款**（显示层，只改观感，不进导出）。
//
// 三条分寸（与排版那几项同一条）：
// - **不内置任何字体文件**：只用系统字体栈；机器上没有那款，浏览器自己回退；
// - **预设是码、不是字面量**：`serif / sans / kai / yahei / dengxian` 存进核心，
//   字体栈留在这一处（换栈不用动库里的数据）；
// - **自定义字体**存成 `family:<族名>`：名字来自后端读到的"系统里装了的"，
//   这里只负责把它拼进字体栈（名字里带引号也能安全拼）。
import type { Appearance } from "../api/core.ts";

/** 五档预设（顺序即设置里出现的顺序）。 */
export type EditorFontPreset = "serif" | "sans" | "kai" | "yahei" | "dengxian";

export const FONT_PRESETS: ReadonlyArray<{ code: EditorFontPreset; labelKey: string }> = [
  { code: "serif", labelKey: "settings.font.serif" },
  { code: "sans", labelKey: "settings.font.sans" },
  { code: "kai", labelKey: "settings.font.kai" },
  { code: "yahei", labelKey: "settings.font.yahei" },
  { code: "dengxian", labelKey: "settings.font.dengxian" },
];

export const DEFAULT_EDITOR_FONT: EditorFontPreset = "serif";
const FAMILY_PREFIX = "family:";
const FILE_PREFIX = "file:";
/** 自定义字体文件在 CSS 里的族名（壳给的地址注入 `@font-face` 时用同一个名字）。 */
export const CUSTOM_FONT_FAMILY = "YanmoCustomFont";

/** 每档的字体栈（兜底放在最后，装没装都不至于空白）。 */
const STACKS: Record<EditorFontPreset, string> = {
  serif: `"Songti SC", "SimSun", "Source Han Serif SC", serif`,
  sans: `"Microsoft YaHei", "PingFang SC", "Source Han Sans SC", sans-serif`,
  kai: `"KaiTi", "STKaiti", "Kaiti SC", "SimSun", serif`,
  yahei: `"Microsoft YaHei", "Microsoft YaHei UI", "PingFang SC", sans-serif`,
  dengxian: `"DengXian", "Microsoft YaHei", sans-serif`,
};

/** 把库里那个值收成"认识的"：预设码原样，`family:<名字>` / `file:<文件名>` 原样，别的一律回默认。 */
export function asEditorFont(values: Partial<Appearance> | null | undefined): string {
  const raw = values?.editor_font;
  if (typeof raw !== "string") return DEFAULT_EDITOR_FONT;
  if (FONT_PRESETS.some((preset) => preset.code === raw)) return raw;
  if (raw.startsWith(FAMILY_PREFIX) && raw.slice(FAMILY_PREFIX.length).trim() !== "") return raw;
  if (raw.startsWith(FILE_PREFIX) && raw.slice(FILE_PREFIX.length).trim() !== "") return raw;
  return DEFAULT_EDITOR_FONT;
}

/** 自定义字体的族名（不是 `family:` 就返回 null）。 */
export function fontFamilyName(value: string): string | null {
  return value.startsWith(FAMILY_PREFIX) ? value.slice(FAMILY_PREFIX.length) : null;
}

/** 自定义字体文件的文件名（不是 `file:` 就返回 null）。 */
export function fontFileName(value: string): string | null {
  return value.startsWith(FILE_PREFIX) ? value.slice(FILE_PREFIX.length) : null;
}

/** 拼成能直接塞进 CSS 的字体栈（族名里可能带引号，先去掉再包一层）。 */
export function fontStackOf(value: string): string {
  if (fontFileName(value) !== null) {
    return `"${CUSTOM_FONT_FAMILY}", "Songti SC", "SimSun", serif`;
  }
  const family = fontFamilyName(value);
  if (family !== null) {
    const safe = family.replace(/["\\]/g, "").trim();
    if (safe !== "") return `"${safe}", "Songti SC", "SimSun", serif`;
  }
  return STACKS[asEditorFont({ editor_font: value } as Partial<Appearance>) as EditorFontPreset];
}

/** 绑到编辑区（和设置里的示例）身上的那一格变量。 */
export function fontStyle(values: Partial<Appearance> | null | undefined): Record<string, string> {
  return { "--ym-editor-font": fontStackOf(asEditorFont(values)) };
}
