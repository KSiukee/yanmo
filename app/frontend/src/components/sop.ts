// 流程那一屏的两件小事：**默认说法取哪一句**、**新步的稳定 id 怎么起**。
//
// 纯函数（不碰 API、不碰组件），所以能单独测——与 `creator.ts` / `inspire.ts` 同一分工。
// 核心零文案：名字 / 说明 / 检查项文字都是"作者改过就用他的，没改过按 id 去字典取"。

import type { Sop, SopCheck, SopSource, SopStep } from "../api/sop.ts";
import { has, t } from "../locales/index.ts";

/** 一步的名字框里那句占位：内置那几步有字典默认，自己加的没有就用通用的。 */
export function stepNamePlaceholder(step: SopStep): string {
  const key = `sop.step.${step.id}.name`;
  return has(key) ? t(key) : t("sop.name.placeholder");
}

/** 一步的说明框里那句占位（同一套规矩）。 */
export function stepNotePlaceholder(step: SopStep): string {
  const key = `sop.step.${step.id}.note`;
  return has(key) ? t(key) : t("sop.note.placeholder");
}

/** 一个检查项的占位（首版检查项是纯用户数据，没有内置文案）。 */
export function checkPlaceholder(check: SopCheck): string {
  const key = `sop.check.${check.id}`;
  return has(key) ? t(key) : t("sop.checks.placeholder");
}

/** 这份流程是从哪一层来的（核心只给码，句子在这里）。 */
export function sourceLabel(source: SopSource): string {
  return t(`sop.source.${source}`);
}

/** 下一批可用的稳定 id：`custom-1`、`custom-2`…（**不撞已有的**）。 */
export function nextId(prefix: string, taken: string[]): string {
  let n = 1;
  while (taken.includes(`${prefix}-${n}`)) n += 1;
  return `${prefix}-${n}`;
}

/** 一屏读数：几步 / 跳过几步 / 勾了几项（**只是读数，不是达成率**）。 */
export function summary(steps: SopStep[]): string {
  const skipped = steps.filter((step) => step.skipped).length;
  const checks = steps.reduce(
    (total, step) => total + step.checks.filter((check) => check.done).length,
    0,
  );
  return t("sop.summary", { steps: steps.length, skipped, checks });
}

/**
 * 把读回来的步骤抄一份能改的（**写回去的是这一份**）。
 *
 * 深拷一层：界面上改了名字、拖了顺序，都不该动到 `board` 里那份（它是"库里那份"的化身，
 * 出错了还要拿它回显）。
 */
export function cloneSteps(steps: SopStep[]): SopStep[] {
  return steps.map((step) => ({
    ...step,
    checks: step.checks.map((check) => ({ ...check })),
    action: step.action ? { ...step.action } : null,
  }));
}

/** 把步骤封成写回去的那一份（核心只认整份替换）。 */
export function toSop(steps: SopStep[]): Sop {
  return { steps: cloneSteps(steps) };
}
