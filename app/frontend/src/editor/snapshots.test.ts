// 版本历史的测试：**回滚之前必须先保险，比着的那一条没了就别把差异留在屏幕上**。
//
// 这里测的是"设备"而不是"渲染"：给一个替身传输层，看它在各种选择下到底叫了哪些动作。

import { test } from "node:test";
import assert from "node:assert/strict";
import { ref } from "vue";

import type { SnapshotDiff, SnapshotRestoreAck, SnapshotSummary } from "../api/core.ts";
import { snapshotReason, useSnapshots, type SnapshotTransport } from "./snapshots.ts";

function entry(id: number, over: Partial<SnapshotSummary> = {}): SnapshotSummary {
  return {
    id,
    char_count: 100,
    reason: "close",
    pinned: false,
    created_at: 1_700_000_000_000 + id,
    ...over,
  };
}

function diff(added: number, removed: number): SnapshotDiff {
  return { lines: [], added, removed, truncated: false, current_char_count: 120 };
}

function build(initial: SnapshotSummary[], opts: { beforeFails?: boolean } = {}) {
  const calls: string[] = [];
  const restored: SnapshotRestoreAck[] = [];
  let entries = [...initial];
  const transport: SnapshotTransport = {
    list: async (node_id) => {
      calls.push(`list:${node_id}`);
      return entries;
    },
    diff: async (node_id, snapshot_id) => {
      calls.push(`diff:${node_id}:${snapshot_id}`);
      return diff(1, 1);
    },
    keep: async (node_id) => {
      calls.push(`keep:${node_id}`);
      return null;
    },
    drop: async (snapshot_id) => {
      calls.push(`drop:${snapshot_id}`);
      entries = entries.filter((item) => item.id !== snapshot_id);
    },
    restore: async (snapshot_id) => {
      calls.push(`restore:${snapshot_id}`);
      return { node_id: 7, body: "旧稿。", char_count: 3, chars_no_punct: 3, word_count: 3, fingerprint: "abcd" };
    },
  };
  const snapshots = useSnapshots({
    transport,
    nodeId: ref(7),
    beforeRestore: async () => {
      calls.push("flush");
      if (opts.beforeFails) throw new Error("存不下去");
    },
    onRestored: (ack) => {
      calls.push("applied");
      restored.push(ack);
    },
    onError: (message) => calls.push(`error:${message}`),
  });
  return { snapshots, calls, restored, setEntries: (next: SnapshotSummary[]) => (entries = next) };
}

test("版本是怎么来的：核心只给码，句子在字典里", () => {
  assert.equal(snapshotReason("keep"), "手动留的");
  assert.equal(snapshotReason("before_restore"), "回滚前留的底");
  assert.equal(snapshotReason("没见过的码"), "自动留的");
});

test("打开就比最新那一版：第一眼想知道的是刚丢的那点东西", async () => {
  const { snapshots, calls } = build([entry(3), entry(2), entry(1)]);
  snapshots.toggle();
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.deepEqual(calls, ["list:7", "diff:7:3"]);
  assert.equal(snapshots.selected.value, 3);
  assert.ok(snapshots.diff.value);
});

test("内容没变时不留重复版本，但要说清为什么", async () => {
  const { snapshots, calls } = build([entry(3)]);
  await snapshots.keep();
  assert.deepEqual(calls, ["keep:7", "list:7"]);
  assert.match(snapshots.note.value, /一模一样/);
});

test("回滚：先落盘 → 再回滚 → 把正文交回会话层", async () => {
  const { snapshots, calls, restored } = build([entry(3)]);

  await snapshots.restore(entry(3));

  assert.deepEqual(calls, ["flush", "restore:3", "applied", "list:7"]);
  assert.equal(restored[0].body, "旧稿。");
  assert.equal(snapshots.selected.value, null, "回滚完就没什么好比的了");
});

test("存不下去就不覆盖：落盘失败时一个回滚动作都不发", async () => {
  const { snapshots, calls, restored } = build([entry(3)], { beforeFails: true });

  await snapshots.restore(entry(3));

  assert.deepEqual(calls, ["flush", "error:存不下去"]);
  assert.equal(restored.length, 0, "没保险就别动手");
});

test("删掉正在比的那一条：差异跟着收起", async () => {
  const { snapshots, calls } = build([entry(3), entry(2)]);

  await snapshots.compare(entry(3));
  await snapshots.drop(entry(3));

  assert.deepEqual(calls, ["diff:7:3", "drop:3", "list:7"]);
  assert.equal(snapshots.selected.value, null);
  assert.equal(snapshots.diff.value, null);
  assert.equal(snapshots.entries.value.length, 1);
});

test("比着的那一条被自动清理掉之后，差异不留在屏幕上", async () => {
  const { snapshots, setEntries } = build([entry(3)]);

  await snapshots.compare(entry(3));
  setEntries([entry(2)]); // 下一次刷新时那条已经被滚动保留清掉了
  await snapshots.refresh();

  assert.equal(snapshots.selected.value, null);
  assert.equal(snapshots.diff.value, null);
});
