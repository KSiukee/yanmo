// 叩问面板的纯展示逻辑：句子渲染、引力拆解、等多久、来源称呼、输入方式称呼、落点与"这一章"。
//
// 这几条守的都是"静默出错"：键写错会露出键名、槽位缺了会留 `{槽位}`、
// 乘数全列出来会把真正的原因埋掉——都在这里钉住。

import assert from "node:assert/strict";
import test from "node:test";

import {
  asksThisChapter,
  classLabel,
  countForChapter,
  defaultTarget,
  dueLabel,
  inputLabel,
  landLabel,
  levelWord,
  reasonLines,
  renderDraft,
  sourceLabel,
  targetLabel,
} from "./question.ts";
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

test("引力拆解讲人话：说清是什么意思，不摆裸数字", () => {
  const lines = reasonLines(gravity);
  const text = lines.map((line) => `${line.label}：${line.value}`).join("|");
  // 三档说法（gravity 里时机 0.8 / 分量 0.7 / 新颖度 1.0）
  assert.ok(text.includes("正是该问的时候"), text);
  assert.ok(text.includes("一般要紧"), text);
  assert.ok(text.includes("还没问过"), text);
  // 真起作用的乘数用话说，而不是 `×1.5`
  assert.ok(text.includes("你说过这类问题好"), text);
  assert.ok(text.includes("你延后过它"), text);
  // ×1 的那几项不说
  assert.ok(!text.includes("从你记的灵感"), `没起作用就别提：${text}`);
  // **一个裸数字都不许出现**——那几个数字作者看不懂
  assert.ok(!/[0-9]/.test(text), `面板上不该有裸数字：${text}`);
});

test("档位的边界钉在这一处", () => {
  assert.equal(levelWord("flow.level.novelty", 0.75), "还没问过");
  assert.equal(levelWord("flow.level.novelty", 0.74), "问过一两回");
  assert.equal(levelWord("flow.level.novelty", 0.4), "问过一两回");
  assert.equal(levelWord("flow.level.novelty", 0.39), "问过好几回了");
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

test("类别称呼：字典里有就用模板句，没有就原样露键", () => {
  const known = classLabel("chapter.empty_body");
  assert.notEqual(known, "chapter.empty_body", "字典里有这条模板，不该露出键名");
  assert.ok(known.length > 0);
  assert.equal(classLabel("module.own.template"), "module.own.template", "认不出来的键原样露出来（不静默）");
});

test("来源称呼：核心自带给一句人话", () => {
  assert.equal(sourceLabel("core"), "研墨自带");
  assert.equal(sourceLabel("module-x"), "来自 module-x");
});

test("输入方式称呼：三种各有各的说法，认不出来的原样露出来", () => {
  // 不是"来源"那一档：这里说的是**答案怎么打出来的**（文本与输入方式解耦）
  assert.equal(inputLabel("typed"), "键盘打的");
  assert.equal(inputLabel("voice"), "口述转的");
  assert.equal(inputLabel("mixed"), "口述加手改的");
  assert.equal(inputLabel("telepathy"), "telepathy", "认不出来的取值不许静默吞掉");
  // 字典缺键时 t() 原样返回键——这里要能一眼看出来是字典漏了
  for (const source of ["typed", "voice", "mixed"]) {
    assert.ok(!inputLabel(source).startsWith("flow."), `字典里缺 flow.input.${source}`);
  }
});

test("落点称呼：两个落点各有各的说法", () => {
  assert.equal(landLabel("cursor"), "落在光标处");
  assert.equal(landLabel("end"), "落在本章末尾");
});

test("认一条问题是不是这一章问出来的——只认 chapter:<当前章> 这一条", () => {
  const asked = { anchors: ["chapter:7", "volume:2"] };
  assert.equal(asksThisChapter(asked, 7), true);
  assert.equal(asksThisChapter(asked, 8), false, "别的章不算");
  assert.equal(asksThisChapter(asked, null), false, "没有当前章时不声称");
  assert.equal(asksThisChapter({ anchors: [] }, 7), false, "没锚点的卡不算这一章的");
  // 只认整条锚点：`chapter:70` 不能被 `chapter:7` 认下来
  assert.equal(asksThisChapter({ anchors: ["chapter:70"] }, 7), false);
  assert.equal(countForChapter([asked, { anchors: ["chapter:7"] }, { anchors: ["chapter:1"] }], 7), 2);
  assert.equal(countForChapter([asked], null), 0);
});

test("默认落点按要素类型分：章纲那一类落章纲，其余落正文", () => {
  assert.equal(defaultTarget("plan"), "outline", "问「这一章发生了什么 / 谁在看」答的就是章纲");
  assert.equal(defaultTarget("chapter"), "body");
  assert.equal(defaultTarget("rhythm"), "body");
  assert.equal(defaultTarget("continuity"), "body");
  assert.equal(defaultTarget(""), "body", "认不出的要素类型按正文算（最不打扰的那一档）");
});

test("落点称呼：三档各有各的说法", () => {
  assert.equal(targetLabel("body"), "正文段落");
  assert.equal(targetLabel("outline"), "章纲");
  assert.equal(targetLabel("scene"), "场景卡");
  for (const target of ["body", "outline", "scene"] as const) {
    assert.ok(!targetLabel(target).startsWith("flow."), `字典里缺 flow.target.${target}`);
  }
});
