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
import { useAppearance } from "./appearance.ts";

function fakeCore(workNaming: string | null = null) {
  const writes: Array<{ work_id: number | null; patch: AppearancePatch }> = [];
  const applied: NamingRewrite[][] = [];
  let plan: NamingRewrite[] = [];
  const view = (naming: string | null): Appearance => ({
    jump_to_end_on_latest: true,
    word_count_caliber: null,
    quote_style: "curly",
    daily_goal: null,
    naming,
    chapter_numbering: "continue",
  });
  return {
    writes,
    applied,
    setPlan(next: NamingRewrite[]) {
      plan = next;
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
