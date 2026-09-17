// 「计划 vs 实际」那一屏的验收：**三档怎么判、能补谁、补完算哪一档**。
//
// 最要紧的一条是"只补不删"：计划里挂了、正文里没认到的人**绝不能**被算进可补名单——
// 那一路要是错了，一次点击就会把作者的计划删掉几条。

import { test } from "node:test";
import assert from "node:assert/strict";

import type { ChapterActualDto, OutlineActualPageDto } from "../api/outline";
import { alignTargets, stateAfterAlign, stateLabelKey, undoRef } from "./actual.ts";

const ref = (id: number, name: string) => ({ entity_id: id, name, matched: name });

const chapter = (over: Partial<ChapterActualDto> = {}): ChapterActualDto => ({
  node_id: 7,
  title: "第一章",
  state: "deviated",
  has_body: true,
  planned: [ref(1, "陆文")],
  matched: [ref(1, "陆文")],
  extra: [ref(2, "老张")],
  missing: [],
  foreshadows: [],
  ...over,
});

test("三档的说法键跟着状态码走", () => {
  assert.equal(stateLabelKey("written"), "actual.state_written");
  assert.equal(stateLabelKey("unwritten"), "actual.state_unwritten");
});

test("可补的只有「正文里有、计划里没有」的那些", () => {
  assert.deepEqual(alignTargets(chapter()), [2]);
  // 计划里挂了、正文没认到的人：**不**进可补名单
  const withMissing = chapter({ extra: [], missing: [ref(9, "老张")] });
  assert.deepEqual(alignTargets(withMissing), []);
});

test("补完之后：还有没认到的人就是偏离，否则算已写", () => {
  assert.equal(stateAfterAlign(chapter()), "written");
  assert.equal(stateAfterAlign(chapter({ missing: [ref(9, "老张")] })), "deviated");
  // 没正文的章（未写）不该被这条规则改口
  assert.equal(stateAfterAlign(chapter({ has_body: false, state: "unwritten" })), "unwritten");
});

test("留底只认那一个节点（没有就 null）", () => {
  const page: OutlineActualPageDto = {
    chapters: [],
    truncated: 0,
    undoable: [{ id: 3, node_id: 7, note: "align_cast", created_at: 1 }],
  };
  assert.equal(undoRef(page, 7)?.id, 3);
  assert.equal(undoRef(page, 8), null);
  assert.equal(undoRef(null, 7), null);
});
