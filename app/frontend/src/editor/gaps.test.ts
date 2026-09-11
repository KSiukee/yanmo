// 删章路标的测试：**问一嘴是核心的三态机，界面只负责摆出来、照答话走**。
//
// 这里测的是"设备"而不是"渲染"：给一个替身传输层，看它在各种选择下到底叫了哪些动作。

import { test } from "node:test";
import assert from "node:assert/strict";
import { ref } from "vue";

import type { ChapterGap } from "../api/core.ts";
import { gapNote, useGaps, type GapTransport } from "./gaps.ts";

function gap(overrides: Partial<ChapterGap> = {}): ChapterGap {
  return {
    node_id: 22,
    parent_id: 3,
    parent_title: "第一卷",
    serial: 2,
    title: "第2章",
    deleted_at: Date.now(),
    word_count: 1234,
    ...overrides,
  };
}

function build(found: ChapterGap | null, failing = false) {
  const calls: string[] = [];
  const transport: GapTransport = {
    check: async (work_id, parent_id) => {
      calls.push(`check:${work_id}:${parent_id ?? "-"}`);
      if (failing) throw new Error("核心不可达");
      return found;
    },
    answer: async (node_id, answer) => {
      calls.push(`answer:${node_id}:${answer}`);
    },
    fill: async (node_id) => {
      calls.push(`fill:${node_id}`);
      return 99;
    },
  };
  const errors: string[] = [];
  const gaps = useGaps({
    transport,
    workId: ref<number | null>(1),
    onError: (message) => errors.push(message),
  });
  return { gaps, calls, errors };
}

test("没得问的时候不弹窗，也不拦着建章", async () => {
  const { gaps, calls } = build(null);
  assert.equal(await gaps.check(null), false);
  assert.equal(gaps.pending.value, null);
  assert.deepEqual(calls, ["check:1:-"], "问的是这一层（根级传 null）");
});

test("该问的时候把空缺摆出来，连它在哪一层一起传下去", async () => {
  const { gaps, calls } = build(gap());
  assert.equal(await gaps.check(3), true);
  assert.equal(gaps.pending.value?.title, "第2章");
  assert.deepEqual(calls, ["check:1:3"]);
});

test("补写：照空缺那一条去建空章，返回新章 id，问过的事就此了结", async () => {
  const { gaps, calls } = build(gap());
  await gaps.check(3);
  const created = await gaps.fill();
  assert.equal(created, 99);
  assert.deepEqual(calls, ["check:1:3", "fill:22"]);
  assert.equal(gaps.pending.value, null, "答完弹窗就该收起来");
});

test("稍后再说 / 不用了：把答复交给核心，然后放行接着建新章", async () => {
  const { gaps, calls } = build(gap());
  await gaps.check(3);
  assert.equal(await gaps.answer("deferred"), true);
  assert.deepEqual(calls, ["check:1:3", "answer:22:deferred"]);
  assert.equal(gaps.pending.value, null);
});

test("没有待答的空缺时什么都不做（防连点）", async () => {
  const { gaps, calls } = build(gap());
  assert.equal(await gaps.fill(), null);
  assert.equal(await gaps.answer("ignored"), false);
  assert.deepEqual(calls, []);
});

test("问不出来就别拦着作者：报错、不弹窗", async () => {
  const { gaps, errors } = build(null, true);
  assert.equal(await gaps.check(null), false);
  assert.deepEqual(errors, ["核心不可达"]);
});

test("答复记不下来就留在原地，别偷偷往下走", async () => {
  const found = gap();
  const errors: string[] = [];
  const gaps = useGaps({
    transport: {
      check: async () => found,
      answer: async () => {
        throw new Error("写不进去");
      },
      fill: async () => 99,
    },
    workId: ref<number | null>(1),
    onError: (message) => errors.push(message),
  });
  await gaps.check(3);
  assert.equal(await gaps.answer("ignored"), false);
  assert.deepEqual(errors, ["写不进去"]);
  assert.equal(gaps.pending.value?.node_id, 22, "答复没记下，弹窗就该还在");
});

test("弹窗那句话把人话摆全：哪儿缺了哪一章、什么时候删的、旧稿多少字", () => {
  const note = gapNote(gap({ deleted_at: Date.now() - 2 * 86_400_000 }));
  assert.match(note, /「第一卷」里缺了第2章/);
  assert.match(note, /2 天前删的/);
  assert.match(note, /1234 字/);
  assert.match(gapNote(gap({ parent_title: null })), /^目录里缺了第2章/, "根级不假装有卷");
});
