// 备份的"什么时候做"：每日首启一次、关窗一次、没目标就别做、失败不打扰。
//
// 这里不测核心（快照/体检/保留在 yanmo-core 有自己的验收），只测界面这一层**触发时机**——
// 自动备份最容易出的错是"做太多"（每次开窗都拷一遍）和"失败还弹窗烦人"。

import { test } from "node:test";
import assert from "node:assert/strict";

import type { BackupConfig, BackupReport, BackupStatus } from "../api/core.ts";
import { localOffsetMinutes, useBackup } from "./backup.ts";

function config(over: Partial<BackupConfig> = {}): BackupConfig {
  return {
    targets: [
      { path: "备份盘/研墨备份", volume_id: "AAAA1111", volume_label: "备份盘", removable: false },
    ],
    keep: 7,
    auto_on_start: true,
    auto_on_close: true,
    tip_dismissed: false,
    ...over,
  };
}

function status(over: Partial<BackupStatus> = {}): BackupStatus {
  const cfg = over.config ?? config();
  return {
    config: cfg,
    volumes: [],
    data_volume_id: "BBBB2222",
    has_other_volume: true,
    should_nudge: false,
    today: "2026-09-13",
    targets: cfg.targets.map((target) => ({
      path: target.path,
      volume_label: target.volume_label,
      last_success: null,
      last_problem: null,
      volume_present: true,
      dir_exists: true,
      gaps: [],
    })),
    ...over,
  };
}

function report(): BackupReport {
  return {
    at: 1,
    stamp: "20260913-0100",
    outcomes: [
      {
        path: "备份盘/研墨备份",
        volume_label: "备份盘",
        status: "written",
        reason: "",
        package: "备份盘/研墨备份/研墨备份-20260913-0100",
        bytes: 1024,
        kept: 1,
        removed: 0,
        fingerprint: "abc",
        at: 1,
      },
    ],
  };
}

function harness(current: BackupStatus) {
  const calls = { runs: 0, writes: [] as BackupConfig[], manuals: [] as boolean[] };
  const transport = {
    status: async () => current,
    write: async (next: BackupConfig) => {
      calls.writes.push(next);
      return next;
    },
    run: async (_tz: number, manual: boolean) => {
      calls.runs += 1;
      calls.manuals.push(manual);
      return report();
    },
  };
  return { calls, state: useBackup({ transport, tzOffsetMinutes: () => 480 }) };
}

test("每日首启：今天还没成功过就做一次，已经做过就不重复", async () => {
  const first = harness(status());
  await first.state.onStart();
  assert.equal(first.calls.runs, 1, "今天没备份过 → 做一次");

  const done = harness(
    status({
      targets: [
        {
          path: "备份盘/研墨备份",
          volume_label: "备份盘",
          last_success: "2026-09-13",
          last_problem: null,
          volume_present: true,
          dir_exists: true,
          gaps: [],
        },
      ],
    }),
  );
  await done.state.onStart();
  assert.equal(done.calls.runs, 0, "今天已经成功过 → 不再做");
});

test("没目标 / 关了自动：都不做（自动备份不该自己找活干）", async () => {
  const noTargets = harness(status({ config: config({ targets: [] }) }));
  await noTargets.state.onStart();
  assert.equal(noTargets.calls.runs, 0);

  const off = harness(status({ config: config({ auto_on_start: false }) }));
  await off.state.onStart();
  assert.equal(off.calls.runs, 0, "作者关掉了就不做");
});

test("关窗：开了自动且有目标才做一次（现状得先读回来）", async () => {
  const on = harness(status());
  await on.state.load(); // 真实流程里：开窗时 onStart 已经读过一次现状
  on.state.onClose();
  assert.equal(on.calls.runs, 1);

  const off = harness(status({ config: config({ auto_on_close: false }) }));
  await off.state.load();
  off.state.onClose();
  assert.equal(off.calls.runs, 0, "作者关掉了就不做");

  const empty = harness(status({ config: config({ targets: [] }) }));
  await empty.state.load();
  empty.state.onClose();
  assert.equal(empty.calls.runs, 0, "没目标就没什么可做");

  const unloaded = harness(status());
  unloaded.state.onClose(); // 现状都没读回来：宁可什么都不做，也别猜
  assert.equal(unloaded.calls.runs, 0);
});

test("「不用了」把提示记成已拒（之后不再自动弹）", async () => {
  const { calls, state } = harness(status({ should_nudge: true }));
  await state.load();
  await state.dismissTip();
  assert.equal(calls.writes.length, 1);
  assert.equal(calls.writes[0].tip_dismissed, true);
  assert.equal(calls.writes[0].targets.length, 1, "拒绝提示不该顺手把目标清掉");
});

test("立即备份：结果留在 lastReport 里；核心报错时记在 error 上而不是吞掉", async () => {
  const ok = harness(status());
  await ok.state.load();
  await ok.state.runNow();
  assert.equal(ok.state.lastReport.value?.outcomes[0].status, "written");

  const failing = useBackup({
    transport: {
      status: async () => status(),
      write: async (c) => c,
      run: async () => {
        throw new Error("快照做不出来");
      },
    },
  });
  await failing.load();
  await failing.runNow();
  assert.match(failing.error.value ?? "", /快照做不出来/);
});

test("两套生命周期：亲手点的那次带 manual=true，机器自己做的那两次带 false", async () => {
  // 这个标记只为一件事：手动那份包**核心永不删**（自动的才按份数轮转）。
  // 界面这一侧唯一要守的就是"如实告诉它这一次是谁要的"——传错了就等于悄悄改了保留策略。
  const mine = harness(status());
  await mine.state.runNow();
  assert.deepEqual(mine.calls.manuals, [true], "「立即备份」是作者亲手点的");

  const start = harness(status());
  await start.state.onStart();
  assert.deepEqual(start.calls.manuals, [false], "每日首启那一次是机器自己做的");

  const closing = harness(status());
  await closing.state.load(); // 关窗那一刻读的是"当前现状"，没读回来它什么都不做
  closing.state.onClose();
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.deepEqual(closing.calls.manuals, [false], "关窗那一次也是机器自己做的");
});

test("时区偏移的口径：JS 给的是「落后 UTC 多少分钟」，核心要的是「超前多少」", () => {
  assert.equal(localOffsetMinutes(new Date()), -new Date().getTimezoneOffset());
});
