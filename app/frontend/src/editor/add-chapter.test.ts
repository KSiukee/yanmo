// 点「+」之后编排的测试：**先问一嘴，再照作者意图走**。
//
// 这里测的是"设备"而不是"渲染"：给替身（gaps / 目录 / 会话），看它在各种选择下到底叫了哪些动作。

import { test } from "node:test";
import assert from "node:assert/strict";
import { ref } from "vue";

import type { ChapterGap } from "../api/core.ts";
import { useAddChapter } from "./add-chapter.ts";
import type { Directory } from "./directory.ts";
import type { Gaps } from "./gaps.ts";
import type { TreeRow } from "./tree.ts";

function gap(): ChapterGap {
  return {
    node_id: 22,
    parent_id: 1,
    parent_title: null,
    serial: 2,
    title: "第2章",
    deleted_at: Date.now(),
    word_count: 0,
  };
}

/** 一行：默认是"章"（能写正文 → 点「+」是"往后插一章"）。 */
function row(overrides: Partial<TreeRow> = {}): TreeRow {
  return {
    id: 5,
    parent_id: 1,
    kind: "chapter",
    title: "第5章",
    word_count: 0,
    has_body: false,
    holds_body: true,
    accepts_children: false,
    depth: 1,
    has_children: false,
    expanded: false,
    chapter_count: 0,
    subtree_word_count: 0,
    ...overrides,
  };
}

/** 一卷：点「+」是"往里加一章"。 */
const volume = () => row({ id: 5, kind: "volume", holds_body: false, accepts_children: true });

function build(options: { found?: ChapterGap | null; fillFails?: boolean; answerFails?: boolean } = {}) {
  const calls: string[] = [];
  const pending = ref<ChapterGap | null>(null);
  const gaps = {
    pending,
    busy: ref(false),
    check: async (parent_id: number | null) => {
      calls.push(`check:${parent_id ?? "-"}`);
      pending.value = options.found ?? null;
      return pending.value !== null;
    },
    fill: async () => {
      calls.push("fill");
      if (options.fillFails) return null;
      pending.value = null;
      return 99;
    },
    answer: async (answer: "deferred" | "ignored") => {
      calls.push(`answer:${answer}`);
      if (options.answerFails) return false;
      pending.value = null;
      return true;
    },
    dismiss: () => {
      calls.push("dismiss");
      pending.value = null;
    },
  } as Gaps;

  const directory = {
    create: async (parent_id: number | null, kind: string) => {
      calls.push(`create:${parent_id ?? "-"}:${kind}`);
      return 7;
    },
    refresh: async () => {
      calls.push("refresh");
    },
  } as Pick<Directory, "create" | "refresh">;

  let next = 7; // 假传输层发号：第一次建成 7，第二次 8……
  const adding = useAddChapter({
    gaps,
    directory,
    openFreshChapter: async (node_id) => {
      calls.push(`fresh:${node_id}`);
    },
    addChapterAfter: async (node_id) => {
      calls.push(`after:${node_id}`);
      return next++;
    },
  });
  return { adding, calls, pending };
}

test("没得问：接着他在那一章后面插一章", async () => {
  const { adding, calls } = build();
  await adding.addHere(row());
  assert.deepEqual(calls, ["check:1", "after:5"], "问的是这一章所在的层（parent=1）");
  assert.equal(adding.visible.value, false);
});

test("没得问：卷上点 + 就是往里加一章并打开", async () => {
  const { adding, calls } = build();
  await adding.addHere(volume());
  assert.deepEqual(calls, ["check:5", "create:5:chapter", "fresh:7"], "问的是这一卷里面（layer=5）");
});

test("有得问：先摆弹窗，这一行先不建章", async () => {
  const { adding, calls } = build({ found: gap() });
  await adding.addHere(row());
  assert.deepEqual(calls, ["check:1"], "只问了一句，没建章");
  assert.equal(adding.visible.value, true);
  assert.equal(adding.gap.value?.title, "第2章");
});

test("补写：补完刷新目录并切过去", async () => {
  const { adding, calls } = build({ found: gap() });
  await adding.addHere(row());
  await adding.fill();
  assert.deepEqual(calls, ["check:1", "fill", "refresh", "fresh:99"]);
  assert.equal(adding.visible.value, false);
});

test("稍后再说：记下答复，再照他原来的意图建章", async () => {
  const { adding, calls } = build({ found: gap() });
  await adding.addHere(row());
  await adding.answer("deferred");
  assert.deepEqual(calls, ["check:1", "answer:deferred", "after:5"]);
  assert.equal(adding.visible.value, false);
});

test("答复记不下来：弹窗留着，也不偷偷建章", async () => {
  const { adding, calls } = build({ found: gap(), answerFails: true });
  await adding.addHere(row());
  await adding.answer("ignored");
  assert.deepEqual(calls, ["check:1", "answer:ignored"]);
  assert.equal(adding.visible.value, true, "没记下就当没答，弹窗还在");
});

test("补写没成：不刷新不切章，弹窗留着", async () => {
  const { adding, calls } = build({ found: gap(), fillFails: true });
  await adding.addHere(row());
  await adding.fill();
  assert.deepEqual(calls, ["check:1", "fill"]);
  assert.equal(adding.visible.value, true);
});

test("去回收站看看 / 点遮罩：只收起来，不记答复", async () => {
  const { adding, calls } = build({ found: gap() });
  await adding.addHere(volume());
  adding.dismiss();
  assert.deepEqual(calls, ["check:5", "dismiss"], "没有 answer —— 下次点 + 还会问");
  assert.equal(adding.visible.value, false);
  assert.equal(adding.gap.value, null);
});

test("连点同一行的 +：新章接着上一次那一章往下排（不是插在同一位置倒着长）", async () => {
  const { adding, calls } = build();
  await adding.addHere(row()); // 第5章那一行
  await adding.addHere(row()); // 还是那一行
  assert.deepEqual(
    calls,
    ["check:1", "after:5", "check:1", "after:7"],
    "第二次锚在刚建出来的 7 上——于是 11、12 这样往下排",
  );
});

test("换一行点 +：位置仍旧是「点哪儿插哪儿」", async () => {
  const { adding, calls } = build();
  await adding.addHere(row()); // 第5章 → 建出 7
  await adding.addHere(row({ id: 6 })); // 换一行（第6章）
  assert.deepEqual(calls, ["check:1", "after:5", "check:1", "after:6"], "另一行就锚它自己");
});
