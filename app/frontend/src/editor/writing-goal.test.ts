// 每日目标：**全局打底 + 每书覆盖**的读写与输入解析。
//
// 这里测两件事：
// 1. `parseGoalInput`——目标输入框那一格的值怎么变成"要写进库的数字"。
//    ⚠️ 必测**数字入参**：`v-model` 挂在 `<input type="number">` 上时 Vue 给回的是数字，
//    曾经当字符串处理（`.trim()`）导致点「存下」抛异常、作者看到的是"设置了不保存"。
// 2. `useDailyGoal` 的读写次序与覆盖语义（疑问一律问核心/替身，界面不自己猜）。

import { test } from "node:test";
import assert from "node:assert/strict";
import { ref } from "vue";

import type { Appearance, AppearancePatch } from "../api/core";
import { parseGoalInput, useDailyGoal } from "./writing-goal.ts";

test("目标解析：数字入参（type=number 的 v-model）不炸，也不丢", () => {
  // ★ 回归：这三条是"设置了不保存"那次的真凶
  assert.equal(parseGoalInput(500), 500);
  assert.equal(parseGoalInput(0), 0);
  assert.equal(parseGoalInput(Number.NaN), 0, "空输入框在 type=number 下是 NaN");
});

test("目标解析：字符串、带空白、小数、负数与认不出来的值", () => {
  assert.equal(parseGoalInput("500"), 500);
  assert.equal(parseGoalInput(" 800 "), 800);
  assert.equal(parseGoalInput("500.9"), 500, "目标只认整字");
  assert.equal(parseGoalInput(""), 0, "清空 = 不设目标");
  assert.equal(parseGoalInput(null), 0);
  assert.equal(parseGoalInput(undefined), 0);
  assert.equal(parseGoalInput("abc"), 0, "认不出来当没设");
  assert.equal(parseGoalInput(-5), 0);
  assert.equal(parseGoalInput("0"), 0);
});

/** 核心那套"覆盖 → 全局"的替身：读回来的永远是合并后的那一份。 */
function fakeCore(globalGoal: number | null, overrides: Record<number, number | null> = {}) {
  const writes: Array<{ work_id: number | null; patch: AppearancePatch }> = [];
  const read = async (work_id: number | null): Promise<Appearance> => {
    const own = work_id === null ? undefined : overrides[work_id];
    return {
      jump_to_end_on_latest: true,
      word_count_caliber: null,
      quote_style: "curly",
      daily_goal: own === undefined ? globalGoal : own,
    };
  };
  const write = async (work_id: number | null, patch: AppearancePatch): Promise<Appearance> => {
    writes.push({ work_id, patch });
    const value = patch.daily_goal ?? 0;
    // 清掉（0 / 负数）＝**把那一层删掉**，与核心一致：删掉才回得到"继承全局"
    if (work_id === null) globalGoal = value > 0 ? value : null;
    else if (value > 0) overrides[work_id] = value;
    else delete overrides[work_id];
    return read(work_id);
  };
  return { read, write, writes };
}

test("设本书目标：写到这本书那一层，写完回读的是它", async () => {
  const core = fakeCore(2000);
  const workId = ref<number | null>(7);
  const daily = useDailyGoal({ read: core.read, write: core.write }, workId);

  await daily.load();
  assert.equal(daily.goal.value, 2000, "最开始继承全局");
  assert.equal(daily.defaultGoal.value, 2000);

  await daily.set(500, "work");
  assert.deepEqual(core.writes, [{ work_id: 7, patch: { daily_goal: 500 } }]);
  assert.equal(daily.goal.value, 500);
  assert.equal(daily.defaultGoal.value, 2000, "动本书不该改默认");

  // 清掉这一层（写 0）= 回到继承全局
  await daily.set(0, "work");
  assert.deepEqual(core.writes[1], { work_id: 7, patch: { daily_goal: 0 } });
  assert.equal(daily.goal.value, 2000);
});

test("设默认目标：写到全局那一层；没打开书时只写这一层", async () => {
  const core = fakeCore(null);
  const workId = ref<number | null>(null);
  const daily = useDailyGoal({ read: core.read, write: core.write }, workId);

  await daily.load();
  assert.equal(daily.goal.value, null, "没设过就没有目标");

  await daily.set(1500, "default");
  assert.deepEqual(core.writes, [{ work_id: null, patch: { daily_goal: 1500 } }]);
  assert.equal(daily.goal.value, 1500, "没打开书时它就是当前生效的那一份");

  // "只设这本书"在没有书的时候不该瞎写（写到 0 号书上去）
  const before = core.writes.length;
  await daily.set(300, "work");
  assert.equal(core.writes.length, before, "没有书就不写");
});
