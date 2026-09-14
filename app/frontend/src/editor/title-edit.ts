// 标题的"宏骨架"与"作者起的名字"怎么分开——**目录里改名只让作者碰名字那一半**。
//
// 为什么要有这一层：号是**位置的函数**——标题里存的是模板（`第{$N}章`），显示时才按同层位置
// 渲染成 `第1章`（见核心的 `numbering`）。所以库里的标题天然带宏：把整串摊进输入框，
// 作者一删就把编号删了，而且满屏 `{$N}` 这种函数代码不该出现在写作界面里
// （真机反馈：改名框里看到 `{$N}章 复活节雕塑`）。
//
// 分工：`prefix` = 宏骨架（原样写回，不给编辑），`name` = 作者的名字（编辑的就是它）。

/** 宏的形状：`{$N}` / `{$N_ZH}` / `{$N:3}` / `{$N_RESET:101}`——都长成 `{$…}`。 */
const MACRO = /\{\$[^}]*\}/;

export interface TitleParts {
  /** 含宏的那一小节 + 跟在它后面的空白；**原样写回，不参与编辑** */
  prefix: string;
  /** 作者起的名字那一段（编辑的就是它） */
  name: string;
}

/**
 * 拆标题。
 *
 * 规矩：**含宏的那一小节连同它后面的空白**算骨架，其余是名字。
 * - `第{$N}章 复活节雕塑` → 骨架 `第{$N}章 `，名字 `复活节雕塑`
 * - `第{$N}章`（还没起名）→ 骨架 `第{$N}章`，名字空
 * - `序章`（本来就不含宏、自起名不占号）→ 骨架空，整串都是名字，作者随便改
 * - 宏写在中间（`灯 第{$N}章 门`）→ 骨架一直算到宏那一小节之后，名字是 `门`：
 *   宁可少给作者可编辑的一段，也不在保存时悄悄把这个宏弄丢（丢宏 = 丢编号）
 */
export function splitTitle(raw: string): TitleParts {
  const text = raw ?? "";
  const hit = MACRO.exec(text);
  if (!hit) return { prefix: "", name: text };
  const after_macro = hit.index + hit[0].length;
  const gap = text.slice(after_macro).search(/\s/);
  const cut = gap < 0 ? text.length : after_macro + gap + 1;
  return { prefix: text.slice(0, cut), name: text.slice(cut) };
}

/** 合标题：骨架原样 + 一个空格 + 名字；名字清空就只剩骨架（编号还在，章名没了）。 */
export function composeTitle(parts: TitleParts, name: string): string {
  const skeleton = parts.prefix.trimEnd();
  const clean = (name ?? "").trim();
  if (!clean) return skeleton;
  return skeleton ? `${skeleton} ${clean}` : clean;
}

/**
 * 输入框前面那行灰字：骨架**渲染后**的样子（`第{$N}章 ` → `第1章`）。
 *
 * 从渲染后的整串里把名字摘掉得来——界面上不该再自己算一遍号（号只在核心算，
 * 见"号 = 位置的函数"）；摘不出来就退回整串，反正只是给作者看一眼的提示。
 */
export function renderedPrefix(rendered: string, parts: TitleParts): string {
  if (!parts.prefix) return "";
  const tail = parts.name.trim();
  if (tail && rendered.endsWith(tail)) {
    return rendered.slice(0, rendered.length - tail.length).trimEnd();
  }
  return rendered.trim();
}
