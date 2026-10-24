// 编辑区那三样**显示层**偏好 → 绑到元素上的属性与变量（一处算清，组件里不再有算法）。
//
// 三样各自纯逻辑的出处（这样件只做"合起来"这一件事）：
//   · 正文排版（字号 / 行距 / 字距）→ `editor/typography.ts`
//   · 稿纸横线（线型 + 一格多高）  → `editor/rule.ts`
//   · 正文背景（底色 / 底纹 / 图片）→ `editor/background.ts`
//
// 为什么合成一件：`EditorPane.vue` 已经贴着 400 行的上帝棘轮，而这三样各写四五行的
// `computed` 本来就不属于"视图"——它们是**同一件事**（把作者的观感偏好翻成样式），
// 放在一处才好一眼看全"编辑区身上到底绑了哪几样东西"。
//
// 一条边界：往里传的 `values` 是**这本书生效的那一份**（每书覆盖优先），
// 组件那边负责取哪一份；这里只管换算。

import type { Appearance } from "../api/core.ts";
import type { BackgroundOutcome } from "../api/background.ts";
import { typographyStyle } from "./typography.ts";
import { asRuleStyle, ruleStyle } from "./rule.ts";
import { asBgColor, asBgLayout, backgroundStyle, bgImageName } from "./background.ts";

/** 编辑区身上要绑的全部东西：两个属性 + 三组 CSS 变量。 */
export interface EditorSurface {
  /** 绑给 `:style` 的那几组变量（顺序即"后一组可以盖前一组"） */
  style: Record<string, string>[];
  /** `data-rule`：稿纸线型（样式表按它决定画不画、画哪一种线） */
  rule: string;
  /** `data-bg`：正文底色（样式表按它挑一根色板变量） */
  bg: string;
  /** `data-bg-layout`：正文底纹（样式表按它铺哪一种纹路） */
  bgLayout: string;
}

/**
 * 把三样偏好翻成编辑区要的属性与变量。
 *
 * `image` 是**壳读回来的那份回执**（见 `api/background.ts`）：界面手里只有一段
 * `data:` URL，**永远没有路径**——所以图那一段变量只在"库里记着有图 **且** 壳真的读回来了"
 * 时才绑上去；读不回来（被删 / 搬走）就整段不绑，正文安静地回到底色。
 */
export function editorSurface(
  values: Partial<Appearance> | null | undefined,
  image: BackgroundOutcome | null,
): EditorSurface {
  const url = image?.data_url ?? null;
  const layer: Record<string, string> =
    bgImageName(values) !== null && url !== null ? { "--ym-bg-image": `url(${url})` } : {};
  return {
    style: [
      typographyStyle(values), // 字号 / 行距 / 字距
      ruleStyle(values), // 稿纸横线一格的格距
      backgroundStyle(values), // 底纹一格的大小 + 图片层的不透明度与铺法
      layer, // 图本身（没有就整段不绑）
    ],
    rule: asRuleStyle(values),
    bg: asBgColor(values),
    bgLayout: asBgLayout(values),
  };
}
