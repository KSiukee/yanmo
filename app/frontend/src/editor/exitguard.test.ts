// 关窗闸门的逻辑测试：**存不下去就别想走**，以及三条补救路径都要真的通。
//
// 全部用替身（假编辑器控制器 + 记账的假命令），不碰真核心，跑得飞快。

import { test } from "node:test";
import assert from "node:assert/strict";

import type { Autosave, AutosaveState } from "./autosave.ts";
import { ExitGate, isSafeToExit, type ExitGateDeps } from "./exitguard.ts";

function stateOf(status: AutosaveState["status"], detail = ""): AutosaveState {
  return { status, detail, char_count: 0, chars_no_punct: 0, word_count: 0, incident: null };
}

/** 编辑器控制器替身：只实现闸门会用到的三样东西。 */
function autosaveStub(initial: AutosaveState, afterFlush?: AutosaveState) {
  let current = { ...initial };
  const stub = {
    node_id: 7,
    async flush() {
      if (afterFlush) current = { ...afterFlush };
    },
    state: () => current,
  };
  return {
    stub: stub as unknown as Autosave,
    setState: (next: AutosaveState) => {
      current = { ...next };
    },
  };
}

/** 记账的假外部动作。 */
function recorder() {
  const calls: string[] = [];
  return {
    calls,
    closeSession: async (node_id: number) => {
      calls.push(`close:${node_id}`);
    },
    abandonSession: async () => {
      calls.push("abandon");
    },
    exitApp: async () => {
      calls.push("exit");
    },
    escapeExport: async (node_id: number, body: string) => {
      calls.push(`escape:${node_id}:${body}`);
      return "逃生目录/1-第一章.txt";
    },
    currentBody: () => "手上的正文",
  };
}

function build(autosave: () => Autosave | null, extra: Partial<ExitGateDeps> = {}) {
  const rec = recorder();
  const gate = new ExitGate({
    autosave,
    closeSession: rec.closeSession,
    abandonSession: rec.abandonSession,
    exitApp: rec.exitApp,
    escapeExport: rec.escapeExport,
    currentBody: rec.currentBody,
    ...extra,
  });
  return { gate, calls: rec.calls };
}

test("已经落盘：放行，并留下关窗快照", async () => {
  const a = autosaveStub(stateOf("saved"), stateOf("saved"));
  const { gate, calls } = build(() => a.stub);

  assert.equal(await gate.requestExit(), "allowed");
  assert.deepEqual(calls, ["close:7", "exit"], "放行前应当先收尾（关窗快照）再退出");
});

test("还有改动没落盘：拦住，不放行", async () => {
  const a = autosaveStub(stateOf("pending"), stateOf("pending"));
  const { gate, calls } = build(() => a.stub);

  assert.equal(await gate.requestExit(), "blocked");
  assert.deepEqual(calls, [], "拦住时不该退出，也不该留下关窗快照");
  assert.equal(gate.state_().blocked, true);
  assert.match(gate.state_().message, /没有落盘/);
});

test("落盘失败：拦住，并把失败原因说清楚", async () => {
  const a = autosaveStub(stateOf("error", "磁盘写入失败"), stateOf("error", "磁盘写入失败"));
  const { gate } = build(() => a.stub);

  assert.equal(await gate.requestExit(), "blocked");
  assert.match(gate.state_().message, /磁盘写入失败/);
});

test("拦住之后重试成功：放行", async () => {
  // 这次让 flush 维持现状，靠 setState 模拟"用户重试之后真的存上了"
  const a = autosaveStub(stateOf("pending"));
  const { gate, calls } = build(() => a.stub);

  assert.equal(await gate.requestExit(), "blocked");
  a.setState(stateOf("saved"));
  assert.equal(await gate.retry(), "allowed");
  assert.deepEqual(calls, ["close:7", "exit"]);
});

test("导出逃生：拿到路径，但仍然不放行（等用户决定）", async () => {
  const a = autosaveStub(stateOf("pending"), stateOf("pending"));
  const { gate, calls } = build(() => a.stub);

  assert.equal(await gate.requestExit(), "blocked");
  const path = await gate.escape();

  assert.equal(path, "逃生目录/1-第一章.txt");
  assert.ok(calls.includes("escape:7:手上的正文"), `逃生导出要带上手上这份正文：${calls.join(" | ")}`);
  assert.equal(gate.state_().escapePath, path);
  assert.equal(calls.includes("exit"), false, "导出成功不等于可以走");
});

test("仍然退出：标记干净退出后走人", async () => {
  const a = autosaveStub(stateOf("pending"), stateOf("pending"));
  const { gate, calls } = build(() => a.stub);

  await gate.requestExit();
  assert.equal(await gate.forceExit(), "allowed");
  assert.deepEqual(calls, ["abandon", "exit"], "强制退出只标记干净，不写关窗快照");
});

test("界面还没就绪：没什么可丢的，直接放行", async () => {
  const { gate, calls } = build(() => null);
  assert.equal(await gate.requestExit(), "allowed");
  assert.deepEqual(calls, ["exit"]);
});

test("落盘永不返回（卡死）：照样拦住并弹对话框，不把关窗请求挂住", async () => {
  // 失效模式（2026-09-15 代码质量评审：中等 14）：闸门 `await autosave.flush()`，
  // 一次永不返回的落盘会让 requestExit 也永不返回——对话框根本弹不出来，窗口像死了一样。
  let flushCalls = 0;
  const stuck = {
    node_id: 7,
    flush: () => {
      flushCalls += 1;
      return new Promise<void>(() => {});
    },
    state: () => stateOf("saving"),
  };
  const { gate, calls } = build(() => stuck as unknown as Autosave, { flushDeadlineMs: 5 });

  assert.equal(await gate.requestExit(), "blocked");
  assert.equal(flushCalls, 1, "仍然要真的去逼一次落盘");
  assert.equal(gate.state_().blocked, true, "必须拦住——对话框靠这个标志弹出来");
  assert.match(gate.state_().message, /没有回应/);
  assert.deepEqual(calls, [], "拦住时不该退出，也不该留下关窗快照");
});


test("判定口径：只有已落盘（或从没写过）才算安全", () => {
  assert.equal(isSafeToExit(stateOf("saved")), true);
  assert.equal(isSafeToExit(stateOf("idle")), true);
  for (const status of ["pending", "saving", "error", "desync"] as const) {
    assert.equal(isSafeToExit(stateOf(status)), false, `${status} 不该被当成安全`);
  }
});
