// 外观偏好这一片的接线验收：**命名规则写哪一层、改完顺手重算"已有章节要不要换"**。
//
// 真正"换什么"的规则在核心（`numbering` / `store::naming`，那边有验收）；这里只盯界面这一层：
// - 设命名规则时 `target` 决定写全局还是只写这本书（每书覆盖）；
// - 改完规则会**立刻重算**一遍"这本书哪些章会换"，面板上那行提示才是最新的；
// - 执行时把**预览过的那份清单**原样交给核心（不重新算一遍——作者看的就是它）。

import { test } from "node:test";
import assert from "node:assert/strict";
import { ref } from "vue";

import type { Appearance, AppearancePatch, NamingRewrite } from "../api/core";
import type { BackgroundOutcome } from "../api/background";
import { useAppearance } from "./appearance.ts";

function fakeCore(workNaming: string | null = null) {
  const writes: Array<{ work_id: number | null; patch: AppearancePatch }> = [];
  const applied: NamingRewrite[][] = [];
  const picks: number[] = [];
  let plan: NamingRewrite[] = [];
  let picture: BackgroundOutcome = {
    name: null,
    data_url: null,
    bytes: null,
    present: false,
    cancelled: false,
  };
  const view = (naming: string | null): Appearance => ({
    jump_to_end_on_latest: true,
    word_count_caliber: null,
    quote_style: "curly",
    daily_goal: null,
    naming,
    chapter_numbering: "continue",
    break_reminder: true,
    break_minutes: 45,
    break_words: 1500,
  });
  return {
    writes,
    applied,
    picks,
    setPlan(next: NamingRewrite[]) {
      plan = next;
    },
    setPicture(next: BackgroundOutcome) {
      picture = next;
    },
    transport: {
      read: async (work_id: number | null) => view(work_id === null ? "arabic" : workNaming),
      write: async (work_id: number | null, patch: AppearancePatch) => {
        writes.push({ work_id, patch });
        return view(patch.naming ?? null);
      },
      reset: async () => view(null),
      previewNaming: async () => plan,
      applyNaming: async (_work_id: number, rewrites: NamingRewrite[]) => {
        applied.push(rewrites);
        return rewrites.length;
      },
      backgroundImage: async () => picture,
      backgroundPick: async () => {
        picks.push(1);
        return picture;
      },
      backgroundClear: async () => {
        picks.push(-1);
        return { name: null, data_url: null, bytes: null, present: false, cancelled: false };
      },
    },
  };
}

test("设命名规则：写哪一层由 target 决定", async () => {
  const core = fakeCore();
  const workId = ref<number | null>(7);
  const state = useAppearance({ transport: core.transport, workId });

  await state.setNaming("chinese", "default");
  await state.setNaming("padded", "work");
  assert.deepEqual(core.writes, [
    { work_id: null, patch: { naming: "chinese" } },
    { work_id: 7, patch: { naming: "padded" } },
  ]);

  // 没打开书时"只设这本书"不该瞎写
  workId.value = null;
  await state.setNaming("arabic", "work");
  assert.equal(core.writes.length, 2, "没有书就不写");
});

test("改完规则立刻重算清单；执行时照预览那份交上去", async () => {
  const core = fakeCore();
  const plan: NamingRewrite[] = [
    { node_id: 1, kind: "chapter", before: "第{$N}章 灯", after: "第{$N_ZH}章 灯" },
    { node_id: 2, kind: "chapter", before: "第{$N}章 门", after: "第{$N_ZH}章 门" },
  ];
  core.setPlan(plan);
  const workId = ref<number | null>(3);
  const state = useAppearance({ transport: core.transport, workId });

  await state.setNaming("chinese", "work");
  assert.deepEqual(state.namingPlan.value, plan, "改完规则清单就是最新的");

  const changed = await state.applyNaming();
  assert.equal(changed, 2);
  assert.deepEqual(core.applied, [plan], "交上去的正是预览过的那一份");
  assert.equal(state.namingPlan.value, null, "执行完清单收起来（面板上的提示跟着消失）");
});

test("没有要换的章：执行是空操作", async () => {
  const core = fakeCore();
  core.setPlan([]);
  const workId = ref<number | null>(3);
  const state = useAppearance({ transport: core.transport, workId });
  await state.previewNaming();
  assert.equal(await state.applyNaming(), null);
  assert.equal(core.applied.length, 0, "空清单不该惊动核心");
});

test("设章节编号方式：同样由 target 决定写哪一层，且不必预览", async () => {
  const core = fakeCore();
  const workId = ref<number | null>(7);
  const state = useAppearance({
    transport: core.transport,
    workId,
    onError: () => {},
  });

  await state.setChapterNumbering("per_volume", "work");
  assert.deepEqual(core.writes, [{ work_id: 7, patch: { chapter_numbering: "per_volume" } }]);

  await state.setChapterNumbering("continue", "default");
  assert.deepEqual(core.writes[1], { work_id: null, patch: { chapter_numbering: "continue" } });

  // 没有打开作品时"只设这本书"是空操作：不该把全局悄悄改掉
  workId.value = null;
  await state.setChapterNumbering("per_volume", "work");
  assert.equal(core.writes.length, 2);
});

test("设正文排版：只写这一项（别的项不受影响），传 0 就是清掉回默认", async () => {
  const core = fakeCore();
  const state = useAppearance({
    transport: core.transport,
    workId: ref<number | null>(null),
    onError: () => {},
  });

  await state.setTypography("editor_font_size", 20);
  assert.deepEqual(core.writes[0], { work_id: null, patch: { editor_font_size: 20 } },
    "写全局那一层，且只带这一项（稀疏合并）");

  await state.setTypography("editor_line_height", 0);
  assert.deepEqual(core.writes[1], { work_id: null, patch: { editor_line_height: 0 } },
    "0 = 清掉这一项（核心当没设过）");
});

test("第二栏露哪一块：只写全局那一层，换过就记住", async () => {
  const core = fakeCore();
  const state = useAppearance({
    transport: core.transport,
    workId: ref<number | null>(7),
    onError: () => {},
  });

  await state.setAsideTab("creator");
  assert.deepEqual(core.writes[0], { work_id: null, patch: { aside_tab: "creator" } },
    "写全局那一层（跟人不跟书），且只带这一项");

  // "auto" 是"清掉这一层"（回默认叩问），原样交给核心判
  await state.setAsideTab("auto");
  assert.deepEqual(core.writes[1], { work_id: null, patch: { aside_tab: "auto" } });
});

test("用眼提醒：三项一次写全局那一层（不进每书覆盖）", async () => {
  const core = fakeCore();
  const state = useAppearance({
    transport: core.transport,
    workId: ref<number | null>(7),
    onError: () => {},
  });

  await state.setBreakReminder(false, 30, 800);
  assert.deepEqual(
    core.writes[0],
    { work_id: null, patch: { break_reminder: false, break_minutes: 30, break_words: 800 } },
    "写全局那一层（阈值跟哪本书无关）；开关与两个阈值一次带上",
  );

  // 改一个数也要把另外两项原样带上（界面上它们本来就是一件事的三个数）
  await state.setBreakReminder(true, 45, 1500);
  assert.deepEqual(core.writes[1], {
    work_id: null,
    patch: { break_reminder: true, break_minutes: 45, break_words: 1500 },
  });
});

// ── 正文背景（底色 / 底纹 / 图片）───────────────────────────────────────────

test("背景那三档：都只写全局那一层，`auto` 原样交给核心当「清掉这一层」", async () => {
  const core = fakeCore();
  const state = useAppearance({
    transport: core.transport,
    workId: ref<number | null>(7),
    onError: () => {},
  });

  await state.setBackgroundColor("green");
  await state.setBackgroundLayout("grid");
  await state.setBackgroundFit("tile");
  assert.deepEqual(core.writes.map((entry) => entry.patch), [
    { editor_bg_color: "green" },
    { editor_bg_layout: "grid" },
    { editor_bg_fit: "tile" },
  ]);
  assert.ok(
    core.writes.every((entry) => entry.work_id === null),
    "与排版同一条口径：写全局那一层（每书覆盖的机制留着，界面不暴露）",
  );

  await state.setBackgroundColor("auto");
  assert.deepEqual(core.writes[3].patch, { editor_bg_color: "auto" });
});

test("不透明度：0 是正经取值（图留着但看不见），不许拿它当「清掉」", async () => {
  const core = fakeCore();
  const state = useAppearance({
    transport: core.transport,
    workId: ref<number | null>(7),
    onError: () => {},
  });

  await state.setBackgroundOpacity(0);
  await state.setBackgroundOpacity(60);
  assert.deepEqual(
    core.writes.map((entry) => entry.patch),
    [{ editor_bg_opacity: 0 }, { editor_bg_opacity: 60 }],
    "0 与 60 都原样递过去（清掉这一层由负数表达，见下一行）",
  );

  await state.setBackgroundOpacity(-1);
  assert.deepEqual(core.writes[2].patch, { editor_bg_opacity: -1 }, "负数＝清掉这一层");
});

test("背景图：选图 / 清图都把壳的回执收下来（界面拿到的只有文件名与 data URL）", async () => {
  const core = fakeCore();
  core.setPicture({
    name: "夜航.png",
    data_url: "data:image/png;base64,AAAA",
    bytes: 2048,
    present: true,
    cancelled: false,
  });
  const state = useAppearance({
    transport: core.transport,
    workId: ref<number | null>(7),
    onError: () => {},
  });

  await state.pickBackground();
  assert.equal(state.background.value?.name, "夜航.png");
  assert.equal(state.background.value?.present, true);
  assert.ok(state.background.value?.data_url?.startsWith("data:image/png"), "渲染靠这一段 data URL");

  await state.clearBackground();
  assert.equal(state.background.value?.name, null, "清掉之后就没有图了");
  assert.deepEqual(core.picks, [1, -1], "一次选图、一次清图，各走各自那条命令");
});

test("图被删掉：白跑一趟也要如实说「还在不在」（present 为 false 时界面回纯色）", async () => {
  const core = fakeCore();
  core.setPicture({
    name: "gone.png",
    data_url: null,
    bytes: null,
    present: false,
    cancelled: false,
  });
  const state = useAppearance({
    transport: core.transport,
    workId: ref<number | null>(7),
    onError: () => {},
  });

  await state.refreshBackground();
  assert.equal(state.background.value?.name, "gone.png", "库里还记着它");
  assert.equal(state.background.value?.present, false, "但文件已经不在了——界面据此回纯色");
  assert.equal(state.background.value?.data_url, null);
});

test("读背景图失败：不挡住设置面板（当作没有图，报一句就完了）", async () => {
  const core = fakeCore();
  const failures: string[] = [];
  const state = useAppearance({
    transport: {
      ...core.transport,
      backgroundImage: async () => {
        throw new Error("读不出来");
      },
    },
    workId: ref<number | null>(7),
    onError: (message) => failures.push(message),
  });

  await state.refreshBackground();
  assert.equal(state.background.value, null, "当作没有图，面板别的项照常能用");
  assert.deepEqual(failures, ["读不出来"], "如实报一句");
});
