// 拆章编排的验收：**顺序**与**失手时不装作成功**。
//
// 这里钉的是真机走查逮到的那个坑：撤销时人正站在"即将被删掉的新章"上，
// 必须在**撤销命令之后、切回原章之前**把落盘控制器摘掉——少了那一下，
// 切章纪律的"先落盘、记光标"会撞上「节点不存在或已删除」，整次撤销卡住。
// 顺序写死在断言里（不只是"都调到了"），因为出问题的正是顺序。

import { test } from "node:test";
import assert from "node:assert/strict";
import { ref } from "vue";

import { useChapterSplit, type SplitDeps } from "./chapter-split.ts";
import type { SplitAck, SplitReceipt } from "../api/split.ts";
import type { EditorSnapshot } from "../api/core.ts";

const RECEIPT: SplitReceipt = {
  old_node_id: 2,
  new_node_id: 7,
  snapshot_id: 3,
  kept_chars: 30,
  moved_chars: 33,
  new_title: "第 2 章",
};

function chapter(node_id: number): EditorSnapshot {
  return {
    work_id: 1,
    node_id,
    title: "",
    body: "",
    char_count: 0,
    chars_no_punct: 0,
    word_count: 0,
    work_language: "zh",
    word_caliber: "chars",
    fingerprint: "",
    cursor: null,
    summary: "",
    scene: null,
  } as EditorSnapshot;
}

/** 一套假依赖：把每一步按顺序记进 `log`，并允许指定哪一步失手。 */
function harness(options: { switchOk?: boolean; flushFails?: boolean; splitFails?: boolean; undoFails?: boolean; busyHeld?: boolean } = {}) {
  const log: string[] = [];
  const busy = ref(options.busyHeld ?? false);
  const failures: string[] = [];
  const deps: SplitDeps = {
    currentNode: () => {
      log.push("current");
      return 2;
    },
    flush: async () => {
      log.push("flush");
      if (options.flushFails) throw new Error("落盘失败");
    },
    runSplit: async (_node_id: number, at: number, title: string): Promise<SplitAck> => {
      log.push(`split:${at}:${title}`);
      if (options.splitFails) throw new Error("核心拒了这一刀");
      return { new_chapter: chapter(7), receipt: RECEIPT };
    },
    runUndo: async (receipt: SplitReceipt): Promise<EditorSnapshot> => {
      log.push(`undo:${receipt.new_node_id}`);
      if (options.undoFails) throw new Error("核心拒了撤销");
      return chapter(receipt.old_node_id);
    },
    detachCurrent: () => {
      log.push("detach");
    },
    switchTo: async (snapshot: EditorSnapshot) => {
      log.push(`switch:${snapshot.node_id}`);
      return options.switchOk ?? true;
    },
    settle: async () => {
      log.push("settle");
    },
    busy,
    report: (message: string) => {
      log.push("report");
      failures.push(message);
    },
  };
  const split = useChapterSplit(deps);
  return { split, log, busy, failures, notice: () => split.notice.value };
}

test("拆章的顺序：先落盘 → 命令 → 切到新章 → 收尾", async () => {
  const { split, log, busy } = harness();
  assert.equal(await split.splitAt(2, "小标题"), true);
  assert.deepEqual(log, ["current", "flush", "split:2:小标题", "switch:7", "settle"]);
  assert.deepEqual(split.notice.value, RECEIPT);
  assert.equal(busy.value, false, "跑完要把闸门放开");
});

test("撤销的顺序：先落盘 → 撤销命令 → **摘掉落盘控制器** → 再切回原章", async () => {
  const { split, log } = harness();
  await split.splitAt(2, "");
  log.length = 0;
  assert.equal(await split.undo(), true);
  assert.deepEqual(log, ["flush", "undo:7", "detach", "switch:2", "settle"]);
  assert.equal(split.notice.value, null, "撤过了：把手收掉");
});

test("落盘失败就不许拆（那一刀会切在旧正文上）", async () => {
  const { split, log, failures } = harness({ flushFails: true });
  assert.equal(await split.splitAt(2, ""), false);
  assert.deepEqual(log, ["current", "flush", "report"], "落盘没成就绝不下刀");
  assert.equal(failures.length, 1);
});

test("切不过去时不装作成功：不留那个把手", async () => {
  const { split } = harness({ switchOk: false });
  assert.equal(await split.splitAt(2, ""), false);
  assert.equal(split.notice.value, null, "没切过去就别留把手（那会指向一个没打开的新章）");
});

test("闸门关着时（正在切章），拆与撤销都不动手", async () => {
  const { split, log } = harness({ busyHeld: true });
  assert.equal(await split.splitAt(2, ""), false);
  assert.deepEqual(log, [], "闸门关着连问都不问");
});

test("命令抛错：报出来，且不留下假的把手", async () => {
  const { split, failures } = harness({ splitFails: true });
  assert.equal(await split.splitAt(2, ""), false);
  assert.equal(split.notice.value, null);
  assert.equal(failures.length, 1);
});

test("撤销抛错：报出来，把手留着（还能再试一次）", async () => {
  const { split, log, failures } = harness({ undoFails: true });
  await split.splitAt(2, "");
  log.length = 0;
  assert.equal(await split.undo(), false);
  assert.deepEqual(log, ["flush", "undo:7", "report"]);
  assert.deepEqual(split.notice.value, RECEIPT, "没撤成，把手不能收");
  assert.equal(failures.length, 1);
});
