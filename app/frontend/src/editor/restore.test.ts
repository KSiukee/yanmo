// 从备份恢复的界面这一层：**只测"什么时候叫它"与"叫不叫得动"**。
//
// 核心自己有验收（体检、留底、回滚都在 yanmo-core/tests/restore.rs）；
// 这里守的是三条最容易写错的界面规矩：
// 1. 换库前**必须先落盘**（beforeApply），落不下就不该换；
// 2. 体检不过 / 没选中的时候，按不动；
// 3. 选了一份新来源，上一份的体检结论要被清掉（不能拿别人的结论给这一份背书）。

import { test } from "node:test";
import assert from "node:assert/strict";

import type { RestoreOutcome, RestorePreview, RestoreSources } from "../api/core.ts";
import { useRestore } from "./restore.ts";

function sources(packages = 1): RestoreSources {
  return {
    packages: Array.from({ length: packages }, (_, index) => ({
      path: `备份盘/研墨备份/研墨备份-20260913-01${index}0`,
      stamp: `20260913-01${index}0`,
      created_at: 1_700_000_000_000 + index,
      device: "写字机",
      works: 1,
      chapters: 2,
      words: 1200,
      bytes: 4096,
      data_format_version: 1,
    })),
    roots: ["备份盘/研墨备份"],
    data_dir: "稿子/研墨",
    keep_dir: "稿子/研墨/旧库留底",
  };
}

function preview(over: Partial<RestorePreview> = {}): RestorePreview {
  return {
    source: "备份盘/研墨备份/研墨备份-20260913-0100",
    kind: "package",
    verify: { ok: true, problems: [] },
    created_at: 1_700_000_000_000,
    device: "写字机",
    engine_version: "0.25.0",
    data_format_version: 1,
    source_last_write_at: 1_700_000_000_000,
    source_works: 1,
    source_chapters: 2,
    source_words: 1200,
    source_bytes: 4096,
    live_last_write_at: 1_700_300_000_000,
    live_works: 1,
    live_words: 1500,
    live_readable: true,
    lost_days: 1,
    lost_words: 300,
    is_live_database: false,
    can_restore: true,
    keep_dir: "稿子/研墨/旧库留底",
    ...over,
  };
}

function outcome(): RestoreOutcome {
  return {
    restored_from: "备份盘/研墨备份/研墨备份-20260913-0100/yanmo.db",
    quarantine: "稿子/研墨/旧库留底/20260913-1400",
    bytes: 4096,
    stamp: "20260913-1400",
  };
}

function harness(options: { preview?: RestorePreview; pick?: string | null } = {}) {
  const calls = { flushes: 0, applied: [] as string[], picked: 0 };
  const transport = {
    sources: async () => sources(),
    preview: async () => options.preview ?? preview(),
    apply: async (source: string) => {
      calls.applied.push(source);
      return outcome();
    },
    pick: async () => {
      calls.picked += 1;
      return options.pick ?? null;
    },
  };
  const restore = useRestore({
    transport,
    tzOffsetMinutes: () => 480,
    beforeApply: async () => {
      calls.flushes += 1;
    },
  });
  return { restore, calls };
}

test("打开时读一次来源；读到的东西就是列表", async () => {
  const { restore } = harness();
  assert.equal(restore.sources.value, null, "还没打开之前不显示旧列表");
  await restore.open();
  assert.equal(restore.visible.value, true);
  assert.equal(restore.sources.value?.packages.length, 1);
});

test("选中一份先体检，体检通过才点得动恢复；换库前一定先落盘", async () => {
  const { restore, calls } = harness();
  await restore.open();
  await restore.select("备份盘/研墨备份/研墨备份-20260913-0100");
  assert.equal(restore.preview.value?.can_restore, true);

  await restore.apply();
  assert.deepEqual(calls.applied, ["备份盘/研墨备份/研墨备份-20260913-0100"]);
  assert.equal(calls.flushes, 1, "换库前必须先把手上一章落盘");
  assert.ok(restore.done.value, "成功之后要留着那句交代（窗口马上重启）");
  assert.equal(restore.busy.value, true, "重启之前一直保持忙，别让人点第二下");
});

test("体检不过就不动手——连落盘都不该发生", async () => {
  const bad = preview({ verify: { ok: false, problems: ["《长夜》的成稿内容与清单不符"] }, can_restore: false });
  const { restore, calls } = harness({ preview: bad });
  await restore.open();
  await restore.select("备份盘/研墨备份/研墨备份-20260913-0100");
  await restore.apply();
  assert.deepEqual(calls.applied, []);
  assert.equal(calls.flushes, 0, "不能恢复时就别惊动落盘");
  assert.equal(restore.done.value, null);
});

test("现在的库读不出来（坏库）也要能恢复——不能因为算不出会丢多少就堵死", async () => {
  const wrecked = preview({ live_readable: false, lost_days: 0, lost_words: 0, live_works: 0, live_words: 0 });
  const { restore, calls } = harness({ preview: wrecked });
  await restore.open();
  await restore.select("备份盘/研墨备份/研墨备份-20260913-0100");
  assert.equal(restore.preview.value?.live_readable, false);
  assert.equal(restore.preview.value?.can_restore, true, "算不出会丢多少，不等于不能恢复");
  await restore.apply();
  assert.deepEqual(calls.applied, ["备份盘/研墨备份/研墨备份-20260913-0100"]);
  assert.equal(calls.flushes, 1, "照样先落盘（落不下就不换）");
});

test("没选来源时按不动", async () => {
  const { restore, calls } = harness();
  await restore.open();
  await restore.apply();
  assert.deepEqual(calls.applied, []);
  assert.equal(calls.flushes, 0);
});

test("换一份来源会清掉上一份的体检结论", async () => {
  const { restore } = harness();
  await restore.open();
  await restore.select("备份盘/研墨备份/研墨备份-20260913-0100");
  assert.ok(restore.preview.value);
  // 再选一份时 preview 先被清掉，随后才填上这一次的结论
  const pending = restore.select("备份盘/研墨备份/研墨备份-20260913-0110");
  assert.equal(restore.preview.value, null, "上一份的结论不能挂在这一份上");
  await pending;
  assert.ok(restore.preview.value);
});

test("取消选文件 = 什么都不做；选中了就接着体检", async () => {
  const cancelled = harness({ pick: null });
  await cancelled.restore.open();
  await cancelled.restore.pickDatabase("选择要恢复的库文件", "研墨的库");
  assert.equal(cancelled.calls.picked, 1);
  assert.equal(cancelled.restore.picked.value, null, "取消了就不该有选中项");
  assert.equal(cancelled.restore.preview.value, null);

  const picked = harness({ pick: "别处/yanmo.db" });
  await picked.restore.open();
  await picked.restore.pickDatabase("选择要恢复的库文件", "研墨的库");
  assert.equal(picked.restore.picked.value, "别处/yanmo.db");
  assert.ok(picked.restore.preview.value, "选完要顺手体检一次");
});

test("换库失败：报出来，并且还能再试一次", async () => {
  const calls = { applied: 0, flushes: 0 };
  const restore = useRestore({
    transport: {
      sources: async () => sources(),
      preview: async () => preview(),
      apply: async () => {
        calls.applied += 1;
        throw new Error("backup.restore_swap_failed");
      },
      pick: async () => null,
    },
    tzOffsetMinutes: () => 480,
    beforeApply: async () => {
      calls.flushes += 1;
    },
  });
  await restore.open();
  await restore.select("备份盘/研墨备份/研墨备份-20260913-0100");
  await restore.apply();
  assert.equal(calls.applied, 1);
  assert.match(restore.error.value ?? "", /restore_swap_failed/);
  assert.equal(restore.busy.value, false, "失败之后要松开，让人能重试");
  assert.equal(restore.done.value, null);

  await restore.apply();
  assert.equal(calls.applied, 2, "第二次仍然该真的去调一次");
});
