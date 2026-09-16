// 大纲表的纯逻辑验收：列、格值、要不要管一下、折叠与筛选、Enter 往哪走。
//
// 这一层没有 DOM：组件只管把它们摆上去。

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  castText,
  cellEditable,
  cellValue,
  COLUMNS,
  DEFAULT_COLUMNS,
  emptyFields,
  foreshadowText,
  needsAttention,
  nextFillableRow,
  visibleRows,
} from "./grid.ts";

function row(over: Partial<Record<string, unknown>> = {}) {
  return {
    node_id: 1,
    parent_id: null,
    kind: "chapter",
    depth: 1,
    title: "第1章",
    summary: "",
    word_count: 0,
    has_body: false,
    fields: { pov: "", goal: "", conflict: "", outcome: "" },
    planted_open: 0,
    collected: 0,
    cast: [],
    ...over,
  } as never;
}

const spec = (key: string) => COLUMNS.find((item) => item.key === key)!;

test("列：默认露章名 + 一句话 + 出场人物 + 四格；伏笔与字数按需勾", () => {
  assert.deepEqual(DEFAULT_COLUMNS, [
    "title",
    "summary",
    "cast",
    "pov",
    "goal",
    "conflict",
    "outcome",
  ]);
  assert.equal(COLUMNS.length, 9);
  assert.equal(spec("summary").editable, true);
  assert.equal(spec("title").editable, false, "章名归目录树管");
  assert.equal(spec("foreshadow").editable, false, "伏笔去伏笔账本改");
  assert.equal(spec("cast").editable, true, "出场人物能改，但改法是点选（不是在格子里打字）");
});

test("卷不承载正文：四格与出场人物对它都填不了（一句话也一样不摆）", () => {
  assert.equal(cellEditable(spec("pov"), { kind: "volume" }), false);
  assert.equal(cellEditable(spec("pov"), { kind: "chapter" }), true);
  assert.equal(cellEditable(spec("pov"), { kind: "scene" }), true);
  assert.equal(cellEditable(spec("cast"), { kind: "volume" }), false);
  assert.equal(cellEditable(spec("cast"), { kind: "chapter" }), true);
  assert.equal(cellEditable(spec("foreshadow"), { kind: "chapter" }), false);
});

test("格值：顶层取顶层，四格从 fields 里取", () => {
  const item = row({ title: "第2章", summary: "他进城", word_count: 1200 });
  (item as { fields: Record<string, string> }).fields = {
    pov: "陆文",
    goal: "",
    conflict: "",
    outcome: "",
  };
  assert.equal(cellValue(item, "title"), "第2章");
  assert.equal(cellValue(item, "summary"), "他进城");
  assert.equal(cellValue(item, "words"), "1200");
  assert.equal(cellValue(item, "pov"), "陆文");
  assert.equal(cellValue(item, "goal"), "");
});

test("四格空着哪几格：只有空白也算没填", () => {
  const item = row();
  (item as { fields: Record<string, string> }).fields = { pov: "  ", goal: "拿到账本", conflict: "", outcome: "" };
  assert.deepEqual(emptyFields(item as never), ["pov", "conflict", "outcome"]);
});

test("要不要管一下：没写一句话、或四格填了一半；四格一个字没填不算", () => {
  // 四格全空 + 没一句话 → 要管（一句话还没写）
  assert.equal(needsAttention(row() as never), true);
  // 四格全空 + 有了一句话 → 不管（"还没打算填四格"，与体检那条规则同一口径）
  assert.equal(needsAttention(row({ summary: "他进城" }) as never), false);
  // 填了一半 → 管
  const half = row({ summary: "他进城" });
  (half as { fields: Record<string, string> }).fields = { pov: "陆文", goal: "", conflict: "", outcome: "" };
  assert.equal(needsAttention(half as never), true);
  // 填全 + 一句话 → 不管
  const done = row({ summary: "他进城" });
  (done as { fields: Record<string, string> }).fields = { pov: "陆文", goal: "拿账本", conflict: "他不给", outcome: "抢到了" };
  assert.equal(needsAttention(done as never), false);
  // 卷：分组行，从不提醒
  assert.equal(needsAttention(row({ kind: "volume" }) as never), false);
});

test("出场人物那一列：名字串起来，一个没有就空着（界面摆「选人物」的提示）", () => {
  assert.equal(castText({ cast: [{ entity_id: 1, name: "陆文" }, { entity_id: 2, name: "老张" }] }), "陆文、老张");
  assert.equal(castText({ cast: [] }), "");
  assert.equal(castText({}), "", "没这一栏（旧形状）也当空着，不许炸");
});

test("伏笔那一列：埋/收都念，一条没有就不摆字", () => {
  assert.equal(foreshadowText({ planted_open: 1, collected: 2 }), "埋 1 / 收 2");
  assert.equal(foreshadowText({ planted_open: 0, collected: 2 }), "收 2");
  assert.equal(foreshadowText({ planted_open: 0, collected: 0 }), "");
});

test("折叠：收起一卷，它下面的行全不摆，到同层或更浅再接着摆", () => {
  const rows = [
    row({ node_id: 10, kind: "volume", depth: 0 }),
    row({ node_id: 11, depth: 1 }),
    row({ node_id: 12, depth: 2, kind: "scene" }),
    row({ node_id: 20, kind: "volume", depth: 0 }),
    row({ node_id: 21, depth: 1 }),
  ];
  const all = visibleRows(rows as never, { collapsed: new Set<number>(), onlyUnfilled: false });
  assert.equal(all.length, 5);
  const folded = visibleRows(rows as never, { collapsed: new Set([10]), onlyUnfilled: false });
  assert.deepEqual(
    folded.map((item) => item.node_id),
    [10, 20, 21],
    "卷本身留着（还能再点开），它下面那一层不摆",
  );
});

test("只看没补的：卷留着当分组头，别的按「要管一下」筛", () => {
  const rows = [
    row({ node_id: 10, kind: "volume", depth: 0 }),
    row({ node_id: 11, summary: "他进城" }), // 四格一字未填：不管
    row({ node_id: 12 }), // 没一句话：管
  ];
  const filtered = visibleRows(rows as never, { collapsed: new Set(), onlyUnfilled: true });
  assert.deepEqual(filtered.map((item) => item.node_id), [10, 12]);
});

test("Enter 往下走：跳过卷那种填不了的行；到头就是 null", () => {
  const rows = [
    row({ node_id: 11, kind: "chapter" }),
    row({ node_id: 10, kind: "volume", depth: 0 }),
    row({ node_id: 12, kind: "chapter" }),
  ];
  assert.equal(nextFillableRow(rows as never, 0, "summary", 1), 2, "跳过卷那一行");
  assert.equal(nextFillableRow(rows as never, 2, "summary", 1), null, "到表尾了");
  assert.equal(nextFillableRow(rows as never, 2, "summary", -1), 0, "Shift+Enter 往上");
});
