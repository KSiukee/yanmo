// 流程那一屏的纯函数验收：默认说法取哪句、新 id 怎么起、读数怎么算、抄副本别动原件。
//
// 与 `creator.test.ts` / `inspire.test.ts` 同一分工：这些函数的输出直接进界面，
// 出错就是屏幕上看得见的毛病（比如名字框里露出一个字典键）。
import { test } from "node:test";
import assert from "node:assert/strict";

import type {
  BookMechFacts,
  ChapterMechFacts,
  MechCastRef,
  SopCheck,
  SopMech,
  SopStep,
} from "../api/sop.ts";
import { parseAsideTab } from "../editor/appearance.ts";
import { t } from "../locales/index.ts";
import {
  MECH_JUDGES,
  MECH_SCOPES,
  checkPlaceholder,
  cloneSteps,
  mechLabel,
  mechReading,
  nextId,
  sourceLabel,
  stepNamePlaceholder,
  stepNotePlaceholder,
  summary,
  toSop,
} from "./sop.ts";

function step(id: string, extra: Partial<SopStep> = {}): SopStep {
  return {
    id,
    name: null,
    note: null,
    minutes: null,
    checks: [],
    action: null,
    skipped: false,
    ...extra,
  };
}

function check(id: string, extra: Partial<SopCheck> = {}): SopCheck {
  return { id, text: null, done: false, mech: null, ...extra };
}

function mechCheck(scope: string, judge: string): SopCheck {
  return check("mech-1", { mech: { scope, judge } });
}

function castRef(name: string): MechCastRef {
  return { entity_id: name.length, name, matched: name };
}

/** 一份"本章"读数（默认：有正文、计划 2 人认到 2 人、无伏笔）。 */
function chapterFacts(extra: Partial<ChapterMechFacts> = {}): ChapterMechFacts {
  return {
    node_id: 7,
    title: "第三章",
    has_body: true,
    planned: 2,
    matched: 2,
    missing: [],
    extra: [],
    planted_open: 0,
    foreshadows: [],
    ...extra,
  };
}

/** 一份"全书"读数（默认：24 章、一样问题都没有）。 */
function bookFacts(extra: Partial<BookMechFacts> = {}): BookMechFacts {
  return {
    chapters: 24,
    truncated: 0,
    cast_missing: 0,
    cast_extra: 0,
    foreshadow: 0,
    planted_open: 0,
    ...extra,
  };
}

test("内置那几步的占位取字典默认；自己加的取通用那句（不露字典键）", () => {
  assert.equal(stepNamePlaceholder(step("core")), t("sop.step.core.name"));
  assert.equal(stepNotePlaceholder(step("revise")), t("sop.step.revise.note"));
  // 自己加的步没有字典条目：必须落回通用那句，绝不能把 `sop.step.custom-1.name` 露出去
  const custom = step("custom-1");
  assert.equal(stepNamePlaceholder(custom), t("sop.name.placeholder"));
  assert.equal(stepNotePlaceholder(custom), t("sop.note.placeholder"));
  assert.ok(!stepNamePlaceholder(custom).includes("sop."));
});

test("检查项占位同一套规矩（首版检查项是纯用户数据，没有内置文案）", () => {
  assert.equal(checkPlaceholder(check("check-1")), t("sop.checks.placeholder"));
  assert.ok(!checkPlaceholder(check("check-1")).includes("sop."));
});

test("这三种来源都有说法（核心只给码）", () => {
  for (const source of ["default", "global", "work"] as const) {
    assert.equal(sourceLabel(source), t(`sop.source.${source}`));
    assert.notEqual(sourceLabel(source), `sop.source.${source}`);
  }
});

test("新 id 从 1 起，且不撞已有的", () => {
  assert.equal(nextId("custom", []), "custom-1");
  assert.equal(nextId("custom", ["custom-1", "custom-2"]), "custom-3");
  assert.equal(nextId("check", ["check-2"]), "check-1", "空号先补");
  assert.equal(nextId("custom", ["other-1"]), "custom-1");
});

test("读数只数数量，不是达成率（跳过与勾选都只是记号）", () => {
  const steps = [
    step("core", { checks: [check("a", { done: true }), check("b")] }),
    step("outline", { skipped: true }),
    step("draft", { checks: [check("c", { done: true })] }),
  ];
  assert.equal(
    summary(steps),
    t("sop.summary", { steps: 3, skipped: 1, checks: 2 }),
  );
});

test("抄一份能改的：改副本不动原件（原件是「库里那份」的化身）", () => {
  const original = [step("core", { name: "定核心", checks: [check("a")], action: null })];
  const copy = cloneSteps(original);
  copy[0].name = "改过了";
  copy[0].checks[0].done = true;
  assert.equal(original[0].name, "定核心", "原件不许被改");
  assert.equal(original[0].checks[0].done, false);
  // 写回去的那一份也是副本（再改一次不影响刚交出去的那份）
  const payload = toSop(original);
  payload.steps[0].name = "又改了";
  assert.equal(original[0].name, "定核心");
});

test("第二栏分区码是闭集，且只此一处解析", () => {
  assert.equal(parseAsideTab("flow"), "flow");
  assert.equal(parseAsideTab("creator"), "creator");
  assert.equal(parseAsideTab("sop"), "sop");
  assert.equal(parseAsideTab("mood"), null, "认不出的给 null，不猜");
  assert.equal(parseAsideTab(""), null);
});

// ── 机械检查项：三档读数 + 挑法 ────────────────────────────────────────────

test("机械项那一行念得出来：每一对（视角 × 判据）都有说法，且不露字典键", () => {
  for (const scope of MECH_SCOPES) {
    for (const judge of MECH_JUDGES) {
      const label = mechLabel({ scope, judge } as SopMech);
      assert.ok(label.includes(t(`sop.mech.scope.${scope}`)), label);
      assert.ok(label.includes(t(`sop.mech.judge.${judge}`)), label);
      assert.ok(!label.includes("sop."), `别把字典键露到界面上：${label}`);
    }
  }
});

test("人工那条没有机械读数（勾不勾是作者自己的记号）", () => {
  assert.equal(mechReading(check("human"), chapterFacts(), bookFacts()), null);
});

test("本章：人对不上就说清缺谁 / 多谁", () => {
  const missing = mechReading(
    mechCheck("chapter", "cast_missing"),
    chapterFacts({ planned: 3, matched: 1, missing: [castRef("沈砚"), castRef("老张")] }),
    null,
  );
  assert.equal(missing?.state, "fail");
  assert.equal(
    missing?.text,
    t("sop.mech.chapter.cast_missing.fail", { count: 2, names: "沈砚、老张" }),
  );

  const extra = mechReading(
    mechCheck("chapter", "cast_extra"),
    chapterFacts({ extra: [castRef("卖炭翁")] }),
    null,
  );
  assert.equal(extra?.state, "fail");
  assert.equal(extra?.text, t("sop.mech.chapter.cast_extra.fail", { count: 1, names: "卖炭翁" }));
});

test("本章：对得上就是过得去，名字多了只摆前三个", () => {
  const pass = mechReading(mechCheck("chapter", "cast_missing"), chapterFacts(), null);
  assert.equal(pass?.state, "pass");
  assert.equal(pass?.text, t("sop.mech.chapter.cast_missing.pass", { planned: 2 }));

  const many = mechReading(
    mechCheck("chapter", "cast_extra"),
    chapterFacts({ extra: ["甲", "乙", "丙", "丁", "戊"].map(castRef) }),
    null,
  );
  const sep = t("common.list_separator");
  assert.equal(
    many?.text,
    t("sop.mech.chapter.cast_extra.fail", {
      count: 5,
      names: `甲${sep}乙${sep}丙` + t("sop.mech.more", { count: 2 }),
    }),
  );
});

test("没法判就说没法判——不猜：没动笔 / 没挂人 / 没埋伏笔 / 读数还没到", () => {
  assert.equal(
    mechReading(mechCheck("chapter", "cast_missing"), chapterFacts({ has_body: false }), null)?.text,
    t("sop.mech.no_body"),
  );
  assert.equal(
    mechReading(mechCheck("chapter", "cast_missing"), chapterFacts({ planned: 0 }), null)?.text,
    t("sop.mech.no_cast"),
  );
  assert.equal(
    mechReading(mechCheck("chapter", "foreshadow"), chapterFacts({ planted_open: 0 }), null)?.text,
    t("sop.mech.no_foreshadow"),
  );
  // 一条还没打开（node_id 为 null）→ 读数没到，**绝不当成通过**
  assert.equal(
    mechReading(mechCheck("chapter", "cast_missing"), null, null)?.state,
    "na",
  );
  assert.equal(mechReading(mechCheck("book", "cast_missing"), null, null)?.text, t("sop.mech.no_book"));
});

test("本章伏笔：有候选才叫点到（候选是「像」，不是「收了」）", () => {
  const pass = mechReading(
    mechCheck("chapter", "foreshadow"),
    chapterFacts({
      planted_open: 3,
      foreshadows: [
        { foreshadow_id: 1, body: "玉佩上的裂纹", fragments: ["玉佩上"], fragments_total: 6 },
      ],
    }),
    null,
  );
  assert.equal(pass?.state, "pass");
  assert.equal(pass?.text, t("sop.mech.chapter.foreshadow.pass", { count: 1 }));

  const fail = mechReading(
    mechCheck("chapter", "foreshadow"),
    chapterFacts({ planted_open: 3 }),
    null,
  );
  assert.equal(fail?.state, "fail");
  assert.equal(fail?.text, t("sop.mech.chapter.foreshadow.fail"));
});

test("全书：数的是「有几章」，不是「有几处」", () => {
  const facts = bookFacts({ cast_missing: 3, cast_extra: 1, foreshadow: 2, planted_open: 4 });
  assert.equal(
    mechReading(mechCheck("book", "cast_missing"), null, facts)?.text,
    t("sop.mech.book.cast_missing.fail", { chapters: 24, count: 3 }),
  );
  assert.equal(
    mechReading(mechCheck("book", "cast_extra"), null, bookFacts())?.state,
    "pass",
  );
  assert.equal(
    mechReading(mechCheck("book", "foreshadow"), null, facts)?.text,
    t("sop.mech.book.foreshadow.pass", { chapters: 24, count: 2 }),
  );
  // 一章都没对到 → 没法判
  assert.equal(
    mechReading(mechCheck("book", "cast_missing"), null, bookFacts({ chapters: 0 }))?.text,
    t("sop.mech.no_chapter_judged"),
  );
});

test("全书：书太大只对到一部分时，读数后面如实带上那句话", () => {
  const reading = mechReading(
    mechCheck("book", "cast_missing"),
    null,
    bookFacts({ cast_missing: 2, truncated: 30 }),
  );
  assert.equal(
    reading?.text,
    t("sop.mech.book.cast_missing.fail", { chapters: 24, count: 2 }) +
      t("sop.mech.truncated", { count: 30 }),
  );
});
