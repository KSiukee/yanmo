// 排版清理的测试：**先看后改**——默认只勾建议那一档、应用前先留底、过期预览绝不硬改。
//
// 这里测的是"设备"而不是"渲染"：给一个替身传输层，看它在各种选择下到底叫了哪些动作。

import { test } from "node:test";
import assert from "node:assert/strict";

import type { TypesetChange, TypesetNotice, TypesetRule } from "../api/core.ts";
import { t } from "../locales/index.ts";
import { groupChanges, useTypeset, type TypesetTransport } from "./typeset.ts";

const RULES: TypesetRule[] = [
  { code: "ellipsis", tier: "safe" },
  { code: "dash", tier: "safe" },
  { code: "repeat_char", tier: "careful" },
  { code: "repeat_punct", tier: "careful" },
  { code: "cjk_latin_space", tier: "style" },
];

function change(rule: string, before: string, after: string): TypesetChange {
  return { rule, before, after, paragraph: 1, context_before: "他走了", context_after: "" };
}

/** 三条命中：一条安全档、一条"看情况"、一条纯风格。 */
const CHANGES: TypesetChange[] = [
  change("ellipsis", "...", "……"),
  change("repeat_punct", "？？", "？"),
  change("cjk_latin_space", "", " "),
];

/** 一处只报告的提醒：引号缺一半。 */
const NOTICES: TypesetNotice[] = [
  {
    rule: "pair_missing",
    mark: "quote_double",
    side: "unclosed",
    paragraph: 2,
    context_before: "他说：",
    context_after: "",
  },
];

const ORIGINAL = "他走了...真的吗？？中文English";

function build(options: { failKeep?: boolean } = {}) {
  const calls: string[] = [];
  const applied: string[] = [];
  const styles: string[] = [];
  let text = ORIGINAL;
  const transport: TypesetTransport = {
    rules: async () => RULES,
    scan: async (value) => {
      calls.push(`scan:${value}`);
      return { changes: CHANGES, notices: NOTICES };
    },
    apply: async (value, ask, accepted) => {
      calls.push(`apply:${accepted.join(",")}:${ask.quote_style}`);
      return `${value}（改过）`;
    },
  };
  const state = useTypeset({
    transport,
    currentText: () => text,
    savedQuoteStyle: () => "corner",
    onQuoteStyle: (style) => styles.push(style),
    beforeApply: async () => {
      calls.push("keep");
      if (options.failKeep) throw new Error("留不下版本");
    },
    onApplied: (fixed) => {
      applied.push(fixed);
      text = fixed;
    },
    onError: (message) => calls.push(`error:${message}`),
  });
  return {
    state,
    calls,
    applied,
    styles,
    setText: (value: string) => {
      text = value;
    },
  };
}

/** 让挂在 `void` 上的重扫落地。 */
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

test("打开只扫不改，默认只勾建议改那一档", async () => {
  const { state, calls } = build();
  await state.open();
  assert.deepEqual(calls, [`scan:${ORIGINAL}`], "打开时只有一次扫描，没有任何写动作");
  assert.equal(state.visible.value, true);
  assert.equal(state.quoteStyle.value, "corner", "引号风格跟着偏好走");
  assert.deepEqual([...state.picked.value], [0], "只勾了安全档那一条");
});

test("整条规则一起勾，也可以单条取消", async () => {
  const { state } = build();
  await state.open();
  state.toggleRule("repeat_punct", true);
  assert.deepEqual([...state.picked.value].sort((a, b) => a - b), [0, 1]);
  state.toggleChange(0);
  assert.deepEqual([...state.picked.value], [1]);
});

test("应用之前先留底，改完把正文交回去", async () => {
  const { state, calls, applied } = build();
  await state.open();
  state.toggleRule("repeat_punct", true);
  await state.apply();
  assert.deepEqual(calls, [
    `scan:${ORIGINAL}`,
    "keep", // 留底必须在 apply 之前：留不下就不该改
    "apply:0,1:corner",
    `scan:${ORIGINAL}（改过）`,
  ]);
  assert.deepEqual(applied, [`${ORIGINAL}（改过）`]);
  assert.equal(state.note.value, t("typeset.done", { count: 2 }));
});

test("预览之后稿子又改过：不拿旧序号改新稿子", async () => {
  const { state, calls, setText } = build();
  await state.open();
  setText(`${ORIGINAL}又打了一段`);
  await state.apply();
  assert.ok(!calls.some((call) => call.startsWith("apply:")), "不该动新稿子");
  assert.ok(!calls.includes("keep"), "也不该先留底");
  assert.equal(state.note.value, t("typeset.stale"));
  assert.equal(calls.at(-1), `scan:${ORIGINAL}又打了一段`, "重新扫一遍让作者再看");
});

test("一处都没勾：一个字都不动", async () => {
  const { state, calls } = build();
  await state.open();
  state.toggleChange(0); // 取消掉默认勾中的那一条
  await state.apply();
  assert.equal(calls.filter((call) => call.startsWith("apply:")).length, 0);
  assert.equal(state.note.value, t("typeset.done_none"));
});

test("留底失败就不改，只把失败交出去", async () => {
  const { state, calls } = build({ failKeep: true });
  await state.open();
  await state.apply();
  assert.ok(calls.includes("error:留不下版本"));
  assert.equal(calls.filter((call) => call.startsWith("apply:")).length, 0);
});

test("换引号风格：记下来并重扫", async () => {
  const { state, calls, styles } = build();
  await state.open();
  state.setQuoteStyle("curly");
  await flush();
  assert.deepEqual(styles, ["curly"], "选了就落库");
  assert.equal(state.quoteStyle.value, "curly");
  assert.equal(calls.filter((call) => call.startsWith("scan:")).length, 2, "建议要跟着新风格重扫");
});

test("分组：只列有命中的规则，顺序照核心给的清单", () => {
  const groups = groupChanges(RULES, CHANGES, new Set([0, 2]));
  assert.deepEqual(groups.map((group) => group.rule.code), ["ellipsis", "repeat_punct", "cjk_latin_space"]);
  assert.deepEqual(groups.map((group) => group.picked), [1, 0, 1]);
  assert.deepEqual(groups.map((group) => group.items.length), [1, 1, 1]);
  assert.equal(groups[2].items[0].index, 2);
});

test("只报告的提醒：列出来但不可勾、也绝不进应用", async () => {
  const { state, calls } = build();
  await state.open();
  assert.deepEqual(state.notices.value, NOTICES, "缺一半的引号要列出来");
  // 提醒不是"改动"：它不占 changes 的位置，也不进勾选集合
  assert.equal(state.changes.value.length, CHANGES.length);
  assert.deepEqual([...state.picked.value], [0]);
  await state.apply();
  // 应用的序号只来自 changes（提醒没有下标可传），所以只有一次、且只带默认勾中的那条
  assert.deepEqual(calls.filter((call) => call.startsWith("apply:")), ["apply:0:corner"]);
});
