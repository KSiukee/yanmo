// 碰撞那一屏的两件小事：**两对怎么说**（关系 / 引子 / 摘要）。
//
// 纯函数（不碰 API、不碰组件），所以能单独测——与 `creator.ts` / `inspire.ts` 同一分工。
// 核心只给**关系码 + 两条原文**，句子全在这里（界面上按语言渲染）。

import type { Collision, CollisionRelation } from "../api/inspire.ts";
import { t } from "../locales/index.ts";

/** 摘要最多取这么长——一屏要摆两条，太长就扫不动了。 */
const EXCERPT_MAX = 60;

/** 一对的规范键（小 id 在前）——草稿、"跳过的那一对"都认它。 */
export function pairKey(pair: Collision): string {
  const left = Math.min(pair.left.id, pair.right.id);
  const right = Math.max(pair.left.id, pair.right.id);
  return `${left}:${right}`;
}

/** 两条是什么关系（核心只给码，句子在这里）。 */
export function relationLabel(relation: CollisionRelation): string {
  return t(`collide.relation.${relation}`);
}

/**
 * **引子**：机制只给这一句骨架，撞出来的那一刻由作者自己写。
 *
 * 两条原文原样嵌进去（作者自己的字），骨架只是一句问话——所以这句话可以直接改、
 * 也可以删光了重写。核心一个字都不生成（它连这句话长什么样都不知道）。
 */
export function collidePrompt(pair: Collision): string {
  return t(`collide.prompt.${pair.relation}`, {
    left: pair.left.body,
    right: pair.right.body,
  });
}

/** 一屏列表里那条摘要：把换行与连续空白压成一个空格，太长就截断。 */
export function excerpt(body: string): string {
  const flat = body.split(/\s+/).filter((piece) => piece.length > 0).join(" ");
  return flat.length > EXCERPT_MAX ? `${flat.slice(0, EXCERPT_MAX)}…` : flat;
}
