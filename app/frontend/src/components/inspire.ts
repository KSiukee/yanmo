// 灵感那一屏的两件小事：**落点怎么说**、**引力凭什么**。
//
// 纯函数（不碰 API、不碰组件），所以能单独测——与 `creator.ts` / `question.ts` 同一分工。
// 核心只给稳定码与乘数，句子在这里。

import type { SparkGravity, SparkTarget } from "../api/inspire.ts";
import { t } from "../locales/index.ts";

/** 场景卡的名字最多取这么长——它是**作者自己的字**，不是核心生成的文案。 */
const TITLE_MAX = 40;

/** 落点的界面说法（闭集与核心一致；核心只给码）。 */
export function targetLabel(target: SparkTarget): string {
  return t(`inspire.target.${target}`);
}

/** 引力拆解那句话：四项都摆出来，"为什么它在前面"一眼看得出。 */
export function gravityLabel(g: SparkGravity): string {
  return t("inspire.gravity", {
    importance: percent(g.importance),
    heat: percent(g.heat),
    relevance: percent(g.relevance),
    novelty: percent(g.novelty),
  });
}

/** 0~1 的乘数换成整数百分比（**只是显示**；乘数本身仍是核心算的那一份）。 */
function percent(value: number): number {
  return Math.round(value * 100);
}

/**
 * 场景卡的名字：拿这条灵感的第一行（作者自己的字）；太长就截断。
 *
 * 空正文给空串——那一路由命名规则接手（与叩问落场景卡时的"名字留给作者"同一条口径）。
 */
export function sceneTitleFrom(body: string): string {
  const first = body.split("\n").map((line) => line.trim()).find((line) => line.length > 0) ?? "";
  return first.length > TITLE_MAX ? `${first.slice(0, TITLE_MAX)}…` : first;
}
