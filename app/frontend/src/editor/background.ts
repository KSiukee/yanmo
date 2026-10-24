// 正文背景那一族（底色 / 底纹 / 图片 + 不透明度）——**纯显示层**：
// 只在编辑区底下垫几层，不进导出、不动正文一个字节。
//
// 四件事在这里合成：
//   ① 作者选的底色与底纹是哪两档（核心 `appearance.editor_bg_*` 的稳定码）；
//   ② 底纹一格的边长——**与正文一行一样高**（字号 × 行距），与稿纸横线同一把尺子
//      （`rule.ts` 的 `ruleStep`）：两份算法不许各写一遍，否则底纹会慢慢从字底下漂开；
//   ③ 背景图怎么铺（铺满 / 装下 / 平铺）；
//   ④ 图片层的不透明度（0~100）。
//
// 颜色本身**一律在样式表里**（`style.css` 的两套色板 + `editor-pane.css` 的配方）：
// 组件只负责说"现在哪一档、透明度多少"，深浅两套配色由 `[data-theme]` 自己认。
// 这样换配色不必惊动任何 JS，也不会出现"JS 里写死了一个浅色底"的裂缝。

import type { Appearance } from "../api/core.ts";
import { ruleStep } from "./rule.ts";

/** 底色（稳定码与核心 `BackgroundColor` 一致；进库存的就是这四个）。 */
export type BgColor = "paper" | "cream" | "green" | "gray";

/** 底纹（稳定码与核心 `BackgroundLayout` 一致）。 */
export type BgLayout = "none" | "grid" | "dots" | "grain";

/** 图片铺法（稳定码与核心 `BackgroundFit` 一致）。 */
export type BgFit = "cover" | "contain" | "tile";

/** 四档底色（面板上的顺序；穷举测试拿它对表）。 */
export const BG_COLORS: readonly BgColor[] = ["paper", "cream", "green", "gray"];

/** 四档底纹（面板上的顺序：不铺 / 方格 / 点阵 / 旧纸）。 */
export const BG_LAYOUTS: readonly BgLayout[] = ["none", "grid", "dots", "grain"];

/** 三档铺法（面板上的顺序）。 */
export const BG_FITS: readonly BgFit[] = ["cover", "contain", "tile"];

/** 默认那几档：**与研墨一直以来的样子一模一样**（纸白、不铺、铺满）——核心那边也是这个默认。 */
export const DEFAULT_BG_COLOR: BgColor = "paper";
export const DEFAULT_BG_LAYOUT: BgLayout = "none";
export const DEFAULT_BG_FIT: BgFit = "cover";
export const DEFAULT_BG_OPACITY = 35;

/** 图片层的不透明度上限（百分比）。 */
export const MAX_BG_OPACITY = 100;

/**
 * 不透明度到多少时提一句"字可能不好认了"——**只是提示，不挡人**。
 *
 * 为什么是 70：再往上，图片的明暗开始跟正文抢注意力；但这是作者的观感，
 * 软件只该说一句、不该替他决定（与"背景太花"那条提示同一个分寸）。
 */
export const BG_OPACITY_HINT = 70;

/** 库里那份（可能是 null / 缺项 / 不认识的码）→ 能用的一档。 */
export function asBgColor(values: Partial<Appearance> | null | undefined): BgColor {
  const raw = values?.editor_bg_color;
  return BG_COLORS.includes(raw as BgColor) ? (raw as BgColor) : DEFAULT_BG_COLOR;
}

export function asBgLayout(values: Partial<Appearance> | null | undefined): BgLayout {
  const raw = values?.editor_bg_layout;
  return BG_LAYOUTS.includes(raw as BgLayout) ? (raw as BgLayout) : DEFAULT_BG_LAYOUT;
}

export function asBgFit(values: Partial<Appearance> | null | undefined): BgFit {
  const raw = values?.editor_bg_fit;
  return BG_FITS.includes(raw as BgFit) ? (raw as BgFit) : DEFAULT_BG_FIT;
}

/** 不透明度：坏值 / 缺项都回默认，越界夹进 0~100（与核心写侧同一条尺子）。 */
export function asBgOpacity(values: Partial<Appearance> | null | undefined): number {
  const raw = values?.editor_bg_opacity;
  if (typeof raw !== "number" || !Number.isFinite(raw)) return DEFAULT_BG_OPACITY;
  return Math.min(MAX_BG_OPACITY, Math.max(0, Math.round(raw)));
}

/** 背景图在数据目录里的**文件名**；没有就是 null（界面手里永远没有路径）。 */
export function bgImageName(values: Partial<Appearance> | null | undefined): string | null {
  const raw = values?.editor_bg_image;
  return typeof raw === "string" && raw.trim() !== "" ? raw : null;
}

/** 铺法 → CSS（`background-size` / `background-repeat` 两件一起说）。 */
export function fitStyle(fit: BgFit): { size: string; repeat: string } {
  if (fit === "contain") return { size: "contain", repeat: "no-repeat" };
  if (fit === "tile") return { size: "auto", repeat: "repeat" };
  return { size: "cover", repeat: "no-repeat" };
}

/**
 * 一条可读性提示：该说哪一句（`null` = 没什么可说的）。
 *
 * 两条判据、都**只提示不挡**：图片层太实（不透明度高）时说"字可能不好认"；
 * 底色不是纸白、又铺了底纹时说"底纹叠在有色底上会更花"。
 * 为什么不做成"禁止"：这是作者看自己正文的观感，软件的职责是说清代价，不是替他决定。
 */
export function readabilityHint(
  values: Partial<Appearance> | null | undefined,
): "opacity" | "busy" | null {
  if (bgImageName(values) === null) return null;
  if (asBgOpacity(values) >= BG_OPACITY_HINT) return "opacity";
  if (asBgColor(values) !== DEFAULT_BG_COLOR && asBgLayout(values) !== DEFAULT_BG_LAYOUT) return "busy";
  return null;
}

/** 绑到编辑区上的那一组 CSS 变量（`--ym-bg-*`）+ 底纹一格的边长。 */
export function backgroundStyle(
  values: Partial<Appearance> | null | undefined,
): Record<string, string> {
  const step = ruleStep(values); // 与稿纸横线**同一把尺子**（字号 × 行距）
  const fit = fitStyle(asBgFit(values));
  return {
    "--ym-bg-step": `${step}px`,
    "--ym-bg-image-opacity": String(asBgOpacity(values) / 100),
    "--ym-bg-image-size": fit.size,
    "--ym-bg-image-repeat": fit.repeat,
  };
}
