// 碰撞那一屏的两件小事：**两头怎么说**（种类 / 关系 / 引子 / 摘要）。
//
// 纯函数（不碰 API、不碰组件），所以能单独测——与 `creator.ts` / `inspire.ts` 同一分工。
// 两头长什么样是**核心算的**（它给规范标识与那段字），这里只把关系码与种类码翻成人话。

import type { Collision, CollisionKind, CollisionSideView } from "../api/inspire.ts";
import { formatWhen } from "../editor/display.ts";
import { t } from "../locales/index.ts";

/** 摘要最多取这么长——一屏要摆两头，太长就扫不动了。 */
const EXCERPT_MAX = 60;

/** 一对的规范键（两头各自的规范标识接起来）——草稿、"跳过的那一对"都认它。 */
export function pairKey(pair: Collision): string {
  return `${pair.left.key}|${pair.right.key}`;
}

/** 这一头是什么（核心只给码，句子在这里）。 */
export function sideLabel(kind: CollisionKind): string {
  return t(`collide.side.${kind}`);
}

/** 两头是什么关系（核心只给码，句子在这里）。 */
export function relationLabel(relation: Collision["relation"]): string {
  return t(`collide.relation.${relation}`);
}

/**
 * **引子**：机制只给这一句骨架，撞出来的那一刻由作者自己写。
 *
 * 两头那段字原样嵌进去（碎片 / 章纲 / 伏笔是正文，人物卡是名字）——所以这句话可以
 * 直接改、也可以删光了重写。核心一个字都不生成（它连这句话长什么样都不知道）。
 */
export function collidePrompt(pair: Collision): string {
  return t(`collide.prompt.${pair.relation}`, {
    left: pair.left.text,
    right: pair.right.text,
  });
}

/**
 * 这一头"多会儿记下的"——只有碎片那一头有（元素不是"记下的"，是"本来就有的"）。
 *
 * 返回**已经渲染好的那句话**（空串 = 这一头没有时刻），模板里 `v-if` 直接用。
 */
export function sideWhenText(side: CollisionSideView): string {
  return side.spark ? formatWhen(side.spark.created_at) : "";
}

/** 一屏列表里那条摘要：把换行与连续空白压成一个空格，太长就截断。 */
export function excerpt(body: string): string {
  const flat = body.split(/\s+/).filter((piece) => piece.length > 0).join(" ");
  return flat.length > EXCERPT_MAX ? `${flat.slice(0, EXCERPT_MAX)}…` : flat;
}
