// 叩问面板的**纯展示逻辑**：草稿怎么渲染成句子、引力怎么讲成人话、等多久怎么算。
//
// 单独成文件的原因：这三件事都是纯函数，能单测；组件只管把它们摆上去。
// 文案一律从字典取（`question.template.*` / `flow.*`）——界面文案只有那一处来源。

import { t } from "../locales/index.ts";
import type { Gravity, QuestionDraft } from "../api/question.ts";

const DAY_MS = 86_400_000;

/**
 * 一条候选渲染成句子：模板键查字典、槽位填进去。
 *
 * 字典里缺键时 `t()` 会**原样返回键**（屏幕上出现 `question.template.xxx`，
 * 一眼看得见），缺槽位时 `fill()` 会把 `{槽位}` 留着并在控制台喊一声——
 * 都不静默，这两种都得当场发现。
 */
export function renderDraft(draft: QuestionDraft): string {
  return t(`question.template.${draft.template_key}`, draft.slots);
}

/** 引力拆解里的一行。 */
export interface ReasonLine {
  label: string;
  value: string;
}

/**
 * 引力拆解讲成人话。
 *
 * 只列**真正在起作用的**乘数：×1 的那几项不列（免得四行 `×1.00` 把真正的原因埋掉）；
 * "在冷却里"另外说一句——那是"为什么它排在后面"最常见的原因。
 */
export function reasonLines(gravity: Gravity): ReasonLine[] {
  const num = (value: number) => value.toFixed(2);
  const lines: ReasonLine[] = [
    { label: t("flow.factor.timeliness"), value: num(gravity.timeliness) },
    { label: t("flow.factor.importance"), value: num(gravity.importance) },
    { label: t("flow.factor.novelty"), value: num(gravity.novelty) },
  ];
  const times = (label: string, factor: number) => {
    if (Math.abs(factor - 1) > 1e-9) lines.push({ label, value: `×${num(factor)}` });
  };
  times(t("flow.factor.derived"), gravity.derived_discount);
  times(t("flow.factor.weight"), gravity.template_weight);
  times(t("flow.factor.defer"), gravity.defer_penalty);
  lines.push({ label: t("flow.factor.total"), value: num(gravity.total) });
  if (gravity.cooled) lines.push({ label: t("flow.factor.cooled"), value: t("flow.state.cooled") });
  return lines;
}

/** 等多久：给一个人类看得懂的说法（面板上那行"3 天后"）。 */
export function dueLabel(due_at_ms: number | null, now_ms: number): string {
  if (due_at_ms === null) return t("flow.waiting.due_unknown");
  const left = due_at_ms - now_ms;
  if (left <= 0) return t("flow.waiting.due_now");
  const days = Math.ceil(left / DAY_MS);
  return t("flow.waiting.due_days", { days });
}

/** 来源怎么称呼：核心自带的说人话，模块提交的照它的名字。 */
export function sourceLabel(source: string): string {
  return source === "core" ? t("flow.source.core") : t("flow.source.module", { name: source });
}
