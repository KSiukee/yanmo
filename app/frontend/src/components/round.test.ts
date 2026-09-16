// 「先问后排版」托盘那点纯逻辑：加一条 / 挪一位 / 删掉 / 改字 / 拼成一段。
//
// 这几条守的都是"静默出错"：同一张卡攒了两条会让"落了没有"说不清、挪到边界外要原样不动、
// 拼段时把空条目也拼进去会在正文里多出空段。——都在这里钉住。

import assert from "node:assert/strict";
import test from "node:test";

import { amendAt, appendRound, moveAt, removeAt, roundHasBody, roundHasText, roundText } from "./round.ts";
import type { AnswerTarget, RoundItem } from "../api/question.ts";

const item = (card_id: number, body: string, target: AnswerTarget | "" = ""): RoundItem => ({
  card_id,
  body,
  target,
  title: "",
});

test("加一条：进末尾；同一张卡再来一次是替换，不是攒两条", () => {
  let items: RoundItem[] = [];
  items = appendRound(items, item(1, "第一段"));
  items = appendRound(items, item(2, "第二段"));
  assert.deepEqual(items.map((one) => one.card_id), [1, 2]);

  items = appendRound(items, item(1, "第一段（改了）"));
  assert.deepEqual(items.map((one) => one.card_id), [2, 1], "同一张卡留在末尾，不出现两条");
  assert.equal(items[1].body, "第一段（改了）");
});

test("挪一位：换位置；挪到边界外原样不动（不报错、也不吞掉那一条）", () => {
  const three = [item(1, "一"), item(2, "二"), item(3, "三")];
  assert.deepEqual(moveAt(three, 0, 1).map((one) => one.card_id), [2, 1, 3]);
  assert.deepEqual(moveAt(three, 2, -1).map((one) => one.card_id), [1, 3, 2]);
  assert.deepEqual(moveAt(three, 0, -1), three, "第一条再往前挪不动");
  assert.deepEqual(moveAt(three, 3, 1), three, "越界的下标当没这回事");
  assert.deepEqual(three.map((one) => one.card_id), [1, 2, 3], "原数组不被改坏");
});

test("删一位与改一位", () => {
  const three = [item(1, "一"), item(2, "二"), item(3, "三")];
  assert.deepEqual(removeAt(three, 1).map((one) => one.card_id), [1, 3]);
  assert.equal(amendAt(three, 2, "三（改了）")[2].body, "三（改了）");
  assert.equal(three[2].body, "三", "原数组不被改坏");
});

test("拼成一段：按顺序、段间空一行、空条目丢掉", () => {
  assert.equal(roundText([item(1, "一"), item(2, "二")]), "一\n\n二");
  assert.equal(roundText([item(1, "  一  "), item(2, "   "), item(3, "三")]), "一\n\n三");
  assert.equal(roundText([]), "");
  assert.equal(roundHasText([]), false);
  assert.equal(roundHasText([item(1, "   ")]), false, "全是空白等于没有");
  assert.equal(roundHasText([item(1, "一句")]), true);
});

test("只有落正文的那几条进正文：章纲与场景卡不混进来", () => {
  const mixed = [item(1, "这一段进正文"), item(2, "这一句进章纲", "outline"), item(3, "这一张是场景卡", "scene")];
  assert.equal(roundText(mixed), "这一段进正文", "章纲与场景卡不进正文");
  assert.equal(roundHasBody(mixed), true);
  assert.equal(roundHasBody([item(1, "只投章纲", "outline")]), false, "全是章纲就别去动编辑器");
  assert.equal(roundText([item(1, "落正文", "body")]), "落正文", "写明白 body 的也算");
});
