// 流程那一屏的纯函数验收：默认说法取哪句、新 id 怎么起、读数怎么算、抄副本别动原件。
//
// 与 `creator.test.ts` / `inspire.test.ts` 同一分工：这些函数的输出直接进界面，
// 出错就是屏幕上看得见的毛病（比如名字框里露出一个字典键）。
import { test } from "node:test";
import assert from "node:assert/strict";

import type { SopCheck, SopStep } from "../api/sop.ts";
import { parseAsideTab } from "../editor/appearance.ts";
import { t } from "../locales/index.ts";
import {
  checkPlaceholder,
  cloneSteps,
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
  return { id, text: null, done: false, ...extra };
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
