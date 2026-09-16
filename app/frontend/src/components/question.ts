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

/** 引力拆解里的一行：**说清"这意味着什么"**，不摆裸数字。 */
export interface ReasonLine {
  label: string;
  value: string;
}

/**
 * 把 0~1 的一个乘数说成**三档人话**（阈值只定在这一处，别处不许再判一遍）。
 *
 * 数字对作者没有意义：`时机紧迫度 0.50` 看不出是早还是晚，而 `正是该问的时候` 一眼就懂。
 * 档位写死在这里，是因为它们是**机制的档位**、不是可调文案——改档位要连着测试一起改。
 */
export function levelWord(key: string, value: number): string {
  if (value >= 0.75) return t(`${key}.high`);
  if (value >= 0.4) return t(`${key}.mid`);
  return t(`${key}.low`);
}

/**
 * 引力拆解讲成人话。
 *
 * 只列**真正在起作用的**乘数：不抬高也不压低的那几项不说（免得几行废话把真正的原因埋掉）；
 * "在冷却里"另外说一句——那是"为什么它排在后面"最常见的原因。
 */
export function reasonLines(gravity: Gravity): ReasonLine[] {
  const lines: ReasonLine[] = [
    { label: t("flow.factor.timeliness"), value: levelWord("flow.level.timeliness", gravity.timeliness) },
    { label: t("flow.factor.importance"), value: levelWord("flow.level.importance", gravity.importance) },
    { label: t("flow.factor.novelty"), value: levelWord("flow.level.novelty", gravity.novelty) },
  ];
  if (gravity.derived_discount < 1) {
    lines.push({ label: t("flow.factor.derived"), value: t("flow.factor.derived.why") });
  }
  if (gravity.template_weight > 1) {
    lines.push({ label: t("flow.factor.weight"), value: t("flow.factor.weight.up") });
  } else if (gravity.template_weight > 0 && gravity.template_weight < 1) {
    lines.push({ label: t("flow.factor.weight"), value: t("flow.factor.weight.down") });
  }
  if (gravity.defer_penalty < 1) {
    lines.push({ label: t("flow.factor.defer"), value: t("flow.factor.defer.why") });
  }
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

/**
 * 一类问题怎么称呼：用它的模板句当名字（`question.template.*`）。
 *
 * 字典里没有这条模板（模块自己提的问题）时，`t()` 会**原样返回键**——
 * 屏幕上出现一个 `some.module.template` 是一眼看得见的错，比空着强。
 */
export function classLabel(template_key: string): string {
  const key = `question.template.${template_key}`;
  const text = t(key);
  // 字典里没有这条模板（模块自己提的问题）：露**模板键本身**，别把查找键整个摆出来
  return text === key ? template_key : text;
}

/** 来源怎么称呼：核心自带的说人话，模块提交的照它的名字。 */
export function sourceLabel(source: string): string {
  return source === "core" ? t("flow.source.core") : t("flow.source.module", { name: source });
}

/**
 * 这一段文字是**怎么打出来的**：`typed` / `voice` / `mixed` 的人话说法。
 *
 * 与 [`sourceLabel`] 是两件事：那个说的是"问题从哪来"，这个说的是"答案怎么进来的"。
 * 认不出的取值**照原样露出来**（屏幕上出现一个 `telepathy` 是一眼看得见的错，比空着强）。
 */
export function inputLabel(source: string): string {
  if (source === "typed") return t("flow.input.typed");
  if (source === "voice") return t("flow.input.voice");
  if (source === "mixed") return t("flow.input.mixed");
  return source;
}

/** 答案落进正文的落点：`cursor` 光标处 / `end` 章末。 */
export type LandAt = "cursor" | "end";

/** 落点怎么称呼（面板上那两个单选）。 */
export function landLabel(at: LandAt): string {
  return at === "cursor" ? t("flow.land.cursor") : t("flow.land.end");
}

/**
 * 一条问题是不是**这一章问出来的**：锚点里有没有 `chapter:<当前章>`。
 *
 * 只认这一条锚点格式（与核心写进 `fragments.linked` 的一致）：认不出就当不是——
 * 拿不准的时候不要声称"这条是问这一章的"，那会把不相干的问题推到作者眼前。
 */
export function asksThisChapter(question: { anchors: string[] }, node_id: number | null): boolean {
  if (node_id === null) return false;
  return question.anchors.includes(`chapter:${node_id}`);
}

/** 候选里与这一章有关的有几条（面板顶上那句话用）。 */
export function countForChapter(questions: { anchors: string[] }[], node_id: number | null): number {
  return questions.filter((question) => asksThisChapter(question, node_id)).length;
}
