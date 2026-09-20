// 流程那一屏的两件小事：**默认说法取哪一句**、**新步的稳定 id 怎么起**。
//
// 纯函数（不碰 API、不碰组件），所以能单独测——与 `creator.ts` / `inspire.ts` 同一分工。
// 核心零文案：名字 / 说明 / 检查项文字都是"作者改过就用他的，没改过按 id 去字典取"。

import type {
  BookMechFacts,
  ChapterMechFacts,
  MechCastRef,
  Sop,
  SopCheck,
  SopMech,
  SopSource,
  SopStep,
} from "../api/sop.ts";
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
    checks: step.checks.map((check) => ({
      ...check,
      mech: check.mech ? { ...check.mech } : null,
    })),
    action: step.action ? { ...step.action } : null,
  }));
}

/** 把步骤封成写回去的那一份（核心只认整份替换）。 */
export function toSop(steps: SopStep[]): Sop {
  return { steps: cloneSteps(steps) };
}

// ── 机械检查项 ────────────────────────────────────────────────────────────
//
// 机械项**是算出来的**，不是勾出来的：核心按当前这一章 / 整本书现算，这里只负责把
// 那点事实念成一句话。三档与人工那条的勾一样好懂：过得去 / 过不去 / 没法判。

/** 机械项的判据白名单（与核心 `MECH_JUDGES` 同一张单子）。 */
export const MECH_JUDGES = ["cast_missing", "cast_extra", "foreshadow"] as const;
/** 机械项的两个视角（与核心 `MECH_SCOPES` 同一张单子）。 */
export const MECH_SCOPES = ["chapter", "book"] as const;

/** 三档：过得去 / 过不去 / **没法判**（还没动笔、没有可判的东西）。 */
export type MechState = "pass" | "fail" | "na";

/** 一条机械项现在的读数：`state` 给颜色，`text` 是已经填好参数的那句话。 */
export interface MechReading {
  state: MechState;
  text: string;
}

/** 机械项那一行怎么念：`本章 · 计划里的人都认到了`。 */
export function mechLabel(mech: SopMech): string {
  return t("sop.mech.label", {
    scope: t(`sop.mech.scope.${mech.scope}`),
    judge: t(`sop.mech.judge.${mech.judge}`),
  });
}

/**
 * 一条检查项现在的读数；`null` = 它不是机械项（人工那条自己勾）。
 *
 * `chapter` / `book` 没取到时说"读数还没到"，**不猜**——猜出来的绿点比没有更糟。
 */
export function mechReading(
  check: SopCheck,
  chapter: ChapterMechFacts | null,
  book: BookMechFacts | null,
): MechReading | null {
  const mech = check.mech;
  if (!mech) return null;
  return mech.scope === "book" ? bookReading(mech.judge, book) : chapterReading(mech.judge, chapter);
}

function chapterReading(judge: string, facts: ChapterMechFacts | null): MechReading {
  if (!facts) return { state: "na", text: t("sop.mech.no_chapter") };
  if (judge === "foreshadow") {
    if (facts.planted_open === 0) return { state: "na", text: t("sop.mech.no_foreshadow") };
    return facts.foreshadows.length > 0
      ? { state: "pass", text: t("sop.mech.chapter.foreshadow.pass", { count: facts.foreshadows.length }) }
      : { state: "fail", text: t("sop.mech.chapter.foreshadow.fail") };
  }
  // 正文是空的：计划里的人当然一个都认不到——那是"还没轮到这一章"，不是"写漏了"
  if (!facts.has_body) return { state: "na", text: t("sop.mech.no_body") };
  if (judge === "cast_extra") {
    return facts.extra.length === 0
      ? { state: "pass", text: t("sop.mech.chapter.cast_extra.pass") }
      : {
          state: "fail",
          text: t("sop.mech.chapter.cast_extra.fail", {
            count: facts.extra.length,
            names: nameList(facts.extra),
          }),
        };
  }
  // 计划里一个人都没挂：这条判据无从谈起（"正文里的人都在计划里"不在此列——那个照样能判）
  if (facts.planned === 0) return { state: "na", text: t("sop.mech.no_cast") };
  return facts.missing.length === 0
    ? { state: "pass", text: t("sop.mech.chapter.cast_missing.pass", { planned: facts.planned }) }
    : {
        state: "fail",
        text: t("sop.mech.chapter.cast_missing.fail", {
          count: facts.missing.length,
          names: nameList(facts.missing),
        }),
      };
}

function bookReading(judge: string, facts: BookMechFacts | null): MechReading {
  if (!facts) return { state: "na", text: t("sop.mech.no_book") };
  if (facts.chapters === 0) return { state: "na", text: t("sop.mech.no_chapter_judged") };
  // 书太大时只对到前几章——这句话要跟着读数一起露，别让人以为全书都看过了
  const tail = facts.truncated > 0 ? t("sop.mech.truncated", { count: facts.truncated }) : "";
  if (judge === "foreshadow") {
    if (facts.planted_open === 0) return { state: "na", text: t("sop.mech.no_foreshadow") };
    return facts.foreshadow > 0
      ? {
          state: "pass",
          text: t("sop.mech.book.foreshadow.pass", { chapters: facts.chapters, count: facts.foreshadow }) + tail,
        }
      : { state: "fail", text: t("sop.mech.book.foreshadow.fail", { chapters: facts.chapters }) + tail };
  }
  const pass = judge === "cast_extra" ? facts.cast_extra === 0 : facts.cast_missing === 0;
  const count = judge === "cast_extra" ? facts.cast_extra : facts.cast_missing;
  const key = `sop.mech.book.${judge}.${pass ? "pass" : "fail"}`;
  return { state: pass ? "pass" : "fail", text: t(key, { chapters: facts.chapters, count }) + tail };
}

/** 摆几个名字就够看了（再多这一行就撑爆了）——多出来的说个数。 */
const NAMES_SHOWN = 3;

function nameList(refs: MechCastRef[]): string {
  const sep = t("common.list_separator");
  const shown = refs.slice(0, NAMES_SHOWN).map((ref) => ref.name).join(sep);
  const rest = refs.length - NAMES_SHOWN;
  return rest > 0 ? shown + t("sop.mech.more", { count: rest }) : shown;
}
