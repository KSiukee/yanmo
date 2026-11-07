// 界面级自测的编排验收：**四件事都量到、口径写清、单步失败不拖垮整段**。
//
// 这里只测编排（动作全注入）——真界面的那一段由 `--self-test-ui` 实跑。

import { test } from "node:test";
import assert from "node:assert/strict";

import { CHAPTER_SWITCHES, INPUT_CHARS, runProbe, type ProbeDeps } from "./acceptance-probe.ts";

function harness(overrides: Partial<ProbeDeps> = {}) {
  const state = {
    switched: [] as number[],
    inserted: "",
    scrolled: 0,
    clock: 0,
  };
  const deps: ProbeDeps = {
    chapters: () => [11, 22],
    openChapter: async (id) => {
      state.switched.push(id);
      state.clock += 5;
    },
    insertText: (text) => {
      state.inserted = text;
      state.clock += 3;
    },
    waitSaved: async () => {
      state.clock += 20;
    },
    scroll: () => {
      state.scrolled += 1;
      state.clock += 2;
    },
    search: async () => {
      state.clock += 40;
    },
    frame: async () => {
      state.clock += 1;
    },
    now: () => state.clock,
    ...overrides,
  };
  return { deps, state };
}

test("四件事都量到，名字与口径都在", async () => {
  const { deps } = harness();
  const steps = await runProbe(deps);

  assert.deepEqual(
    steps.map((step) => step.name),
    ["界面·切章", "界面·输入 100 字→落盘", "界面·滚动一次", "界面·检索（输入→列表）"]
  );
  // 切章：10 次、两章交替、平均 6ms（5+1 帧）
  const switching = steps[0];
  assert.equal(switching.ms, 6);
  assert.match(switching.note, /10 次平均/);
  // 输入：100 字（"测"×100）
  assert.equal(steps[1].ms, 23, "插入 3ms + 落盘 20ms");
  // 检索：含防抖与列表渲染，且注明与命令行口径是两回事
  assert.match(steps[3].note, /防抖/);
  assert.match(steps[3].note, /命令行/);
  assert.ok(steps.every((step) => step.note.length > 0), "每条都要有附注（口径在这里）");
});

test("切章真的切了 10 次，并且两章来回", async () => {
  const { deps, state } = harness();
  await runProbe(deps);

  assert.equal(state.switched.length, CHAPTER_SWITCHES);
  assert.deepEqual(state.switched.slice(0, 4), [11, 22, 11, 22]);
  assert.equal(state.inserted.length, INPUT_CHARS, "输入的是 100 个字");
  assert.equal(state.scrolled, 1, "滚动一次");
});

test("只有一章：切章如实写「没量到」，其余三件照量", async () => {
  const { deps } = harness({ chapters: () => [11] });
  const steps = await runProbe(deps);

  assert.equal(steps.length, 4, "单步量不到不许少一条");
  assert.match(steps[0].note, /没量到/);
  assert.doesNotMatch(steps[1].note, /没量到/);
});

test("某一步抛错：只影响那一条，后面照跑", async () => {
  const { deps } = harness({
    insertText: () => {
      throw new Error("编辑器没挂上");
    },
  });
  const steps = await runProbe(deps);

  assert.equal(steps.length, 4);
  assert.match(steps[1].note, /没量到/);
  assert.match(steps[1].note, /编辑器没挂上/);
  assert.doesNotMatch(steps[2].note, /没量到/, "滚动那一条照常量");
  assert.doesNotMatch(steps[3].note, /没量到/, "检索那一条照常量");
});
