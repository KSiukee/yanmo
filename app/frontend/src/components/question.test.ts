// 叩问面板的纯展示逻辑：句子渲染、引力拆解、等多久、来源称呼。
//
// 这几条守的都是"静默出错"：键写错会露出键名、槽位缺了会留 `{槽位}`、
// 乘数全列出来会把真正的原因埋掉——都在这里钉住。

import assert from "node:assert/strict";
import test from "node:test";

import { dueLabel, reasonLines, renderDraft, sourceLabel } from "./question.ts";
import type { Gravity, QuestionDraft } from "../api/question.ts";

const draft: QuestionDraft = {
  template_key: "chapter.empty_body",
  element: "chapter",
  slots: { chapter: "第三章" },
  anchors: ["chapter:3"],
  importance: 0.7,
};

test("草稿按字典渲染成句子，槽位填上", () => {
  const sentence = renderDraft(draft);
  assert.ok(sentence.includes("第三章"), sentence);
  assert.ok(!sentence.includes("{"), `槽位没填干净：${sentence}`);
  assert.ok(!sentence.startsWith("question.template."), `字典里缺这条模板：${sentence}`);
});

const gravity: Gravity = {
  total: 0.35,
  timeliness: 0.8,
  importance: 0.7,
  novelty: 1,
  derived_discount: 1,
  template_weight: 1.5,
  defer_penalty: 0.5,
  level: "short",
  cooled: false,
};

test("引力拆解只列真正起作用的乘数", () => {
  const lines = reasonLines(gravity);
  const labels = lines.map((line) => line.label).join("|");
  assert.ok(labels.includes("这一类学到的权重"), labels);
  assert.ok(labels.includes("延后降权"), labels);
  assert.ok(!labels.includes("派生折扣"), `×1 的项不该出现：${labels}`);
  const values = lines.map((line) => line.value);
  assert.ok(values.includes("×0.50"), values.join(","));
  assert.ok(values.includes("0.35"), "总引力要给出来");
});

test("在冷却里要单独说一句", () => {
  const lines = reasonLines({ ...gravity, cooled: true });
  assert.ok(
    lines.some((line) => `${line.label}${line.value}`.includes("冷却")),
    JSON.stringify(lines),
  );
});

test("等多久：到点了说到了，没到说几天", () => {
  const now = 1_000 * 86_400_000;
  assert.equal(dueLabel(null, now), "不知道什么时候");
  assert.equal(dueLabel(now - 1, now), "到时候了");
  assert.ok(dueLabel(now + 2 * 86_400_000, now).includes("2"));
});

test("来源称呼：核心自带给一句人话", () => {
  assert.equal(sourceLabel("core"), "研墨自带");
  assert.equal(sourceLabel("module-x"), "来自 module-x");
});
