// 大纲体检这一片的纯逻辑验收：一条发现怎么念、缺的格子怎么称呼、忽略怎么分堆。
//
// 规则本身在核心（那边有单测）；这里只盯"界面这一层摆什么"。

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  anchorTarget,
  fieldLabel,
  isDismissed,
  issueText,
  missingLine,
  readableParams,
  sceneTitle,
  splitIssues,
} from "./outline.ts";
import type { OutlineIssue } from "../api/outline.ts";

function issue(rule: string, params: Record<string, string>, anchors: string[], fingerprint: string): OutlineIssue {
  return { rule, params, anchors, fingerprint };
}

test("一条发现念成人话：取字典、填参数", () => {
  const clash = issue(
    "entity.name_clash",
    { name: "阿文", count: "2", names: "陆文、林昭" },
    ["entity:1", "entity:2"],
    "fp1",
  );
  const text = issueText(clash);
  assert.match(text, /阿文/);
  assert.match(text, /陆文、林昭/);
  assert.doesNotMatch(text, /\{/, "占位符都填上了");
});

test("场景卡那一句：没起名的说人话，不摆一个空的「」", () => {
  const named = issue("scene.missing_fields", { title: "开场", count: "1", missing: "goal" }, ["scene:9"], "fp2");
  assert.match(issueText(named), /开场/);
  assert.equal(sceneTitle(named), "开场");

  const untitled = issue("scene.missing_fields", { title: "", count: "1", missing: "goal" }, ["scene:9"], "fp3");
  assert.equal(sceneTitle(untitled), "（还没起名的场景卡）");
});

test("缺了哪几格：稳定码 → 人话，认不出的照原样露", () => {
  assert.equal(fieldLabel("pov"), "视角");
  assert.equal(fieldLabel("outcome"), "结果");
  assert.equal(fieldLabel("mood"), "mood");
  assert.equal(
    missingLine(issue("scene.missing_fields", { missing: "pov,outcome" }, ["scene:1"], "fp")),
    "缺：视角、结果",
  );
  assert.equal(
    missingLine(issue("scene.missing_fields", { missing: "" }, ["scene:1"], "fp")),
    "",
    "不缺就不摆这一行",
  );
});

test("字典里没有这条规则时照原样露规则码（不静默说成别的）", () => {
  const stranger = issue("scene.looks_wrong", {}, ["scene:1"], "fp");
  assert.equal(issueText(stranger), "scene.looks_wrong");
});

test("分两堆：还要看的 / 已经知道的", () => {
  const open = issue("scene.missing_fields", {}, ["scene:1"], "fp-open");
  const known = issue("scene.missing_fields", {}, ["scene:2"], "fp-known");
  const split = splitIssues([open, known], ["fp-known"]);
  assert.deepEqual(split.open, [open]);
  assert.deepEqual(split.known, [known]);
  assert.equal(isDismissed(known, ["fp-known"]), true);
  assert.equal(isDismissed(open, ["fp-known"]), false);
});

test("点开跳到哪儿：认 entity / scene 锚点", () => {
  assert.deepEqual(anchorTarget(["entity:3"]), {
    kind: "entity",
    id: 3,
  });
  assert.deepEqual(anchorTarget(["scene:9"]), {
    kind: "scene",
    id: 9,
  });
  // 认不出的锚点：点不动（界面据此不摆按钮）
  assert.deepEqual(anchorTarget(["chapter:7"]), { kind: "", id: null });
  assert.deepEqual(anchorTarget([]), { kind: "", id: null });
});

test("一串名字按字典的顿号摆（核心给的是中性分隔）", () => {
  const issue = {
    rule: "entity.name_clash",
    params: { name: "阿文", count: "2", names: "陆文,林昭" },
    anchors: ["entity:1", "entity:2"],
    fingerprint: "fp",
  };
  assert.deepEqual(readableParams(issue).names, "陆文、林昭");
  assert.match(issueText(issue), /陆文、林昭/);
});

test("伏笔锚点：认得出来（体检那条点得动）", () => {
  assert.deepEqual(anchorTarget(["foreshadow:5"]), { kind: "foreshadow", id: 5 });
});

test("故事时间那一栏空着时，用排序值顶一句（句子不该出现一个空的「」）", () => {
  const issue = {
    rule: "timeline.out_of_order",
    params: { body: "他走进来", order: "12", time: "", earlier_body: "开场", earlier_order: "99", earlier_time: "" },
    anchors: ["chapter:5", "chapter:1"],
    fingerprint: "fp",
  };
  const params = readableParams(issue);
  assert.equal(params.time, "第 12 天");
  assert.equal(params.earlier_time, "第 99 天");
  assert.doesNotMatch(issueText(issue), /（）/, "不留空括号");
});
