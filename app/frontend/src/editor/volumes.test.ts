// 分卷编排的验收：**提议只在目录那一栏、收卷能撤销、再等等不是白问**。
//
// 依赖全部注入（不碰界面、不碰核心），所以这里测的是编排本身：
// - 切章之后问一句"这一章后面要不要收卷"，有提议才显示；
// - 「再等等」第一次是"下一章再说"，**连着两次这一卷就不再提**；
// - 「在这里收卷」卷名留空交给核心（按位置渲染成第 N 卷），收完重拉目录、
//   并把光标落到核心顺手起的那一章上；
// - 「撤销」拆掉刚收的那一卷；**慢回来的旧答复不许盖掉新状态**（切章很快）。

import { test } from "node:test";
import assert from "node:assert/strict";

import type { VolumeOffer } from "../api/volumes.ts";
import { useVolumes } from "./volumes.ts";

function offer(container: number | null, count = 24, phase = "early"): VolumeOffer {
  return {
    plan: { target: 30, learned: null, effective: 30 },
    count,
    phase: phase as VolumeOffer["phase"],
    container,
  };
}

function harness(offerFor?: (node_id: number) => Promise<VolumeOffer | null>) {
  const closeCalls: Array<{ node: number; title: string }> = [];
  const dissolved: number[] = [];
  const opened: number[] = [];
  let refreshes = 0;
  const volumes = useVolumes({
    transport: {
      offer: offerFor ?? (async () => offer(1)),
      close: async (node_id, title) => {
        closeCalls.push({ node: node_id, title });
        return { volume_id: 9, opened_chapter: 77, moved: 0 };
      },
      dissolve: async (volume_id) => {
        dissolved.push(volume_id);
        return { moved: 6, merged_into: 1 };
      },
    },
    afterChange: async () => {
      refreshes += 1;
    },
    openChapter: async (node_id) => {
      opened.push(node_id);
    },
  });
  return { volumes, closeCalls, dissolved, opened, refreshes: () => refreshes };
}

test("切到收卷点上才有提议；收卷后重拉目录并把光标落到新卷第一章", async () => {
  const { volumes, closeCalls, opened, refreshes } = harness();
  assert.equal(volumes.offer.value, null, "刚打开还没有提议");

  await volumes.consider(42);
  assert.equal(volumes.offer.value?.count, 24, "核心说这一章是收卷点，界面就提");

  await volumes.closeHere();
  assert.deepEqual(closeCalls, [{ node: 42, title: "" }], "卷名留空＝交给核心按位置渲染成「第 N 卷」");
  assert.deepEqual(volumes.closed.value, { volume_id: 9, count: 24 }, "记住刚收好的那一卷，撤销才有得撤");
  assert.equal(volumes.offer.value, null, "收完不再重复提同一处");
  assert.equal(refreshes(), 1, "结构变了：目录树与卷长口径要跟着重读");
  assert.deepEqual(opened, [77], "空卷里核心顺手起了第一章：光标落过去接着写");
});

test("「再等等」连着两次，这一卷就不再提（回答不能白问）", async () => {
  const { volumes } = harness();
  await volumes.consider(42);
  volumes.snooze();
  assert.equal(volumes.offer.value, null, "先不提了");

  await volumes.consider(43);
  assert.equal(volumes.offer.value?.count, 24, "下一次停下来还问一遍");
  volumes.snooze();
  assert.equal(volumes.offer.value, null);

  await volumes.consider(44);
  assert.equal(volumes.offer.value, null, "第二次「再等等」之后，这一卷闭嘴");
});

test("换到另一卷：问过的记忆不串门", async () => {
  // 第一卷（container = 1）已经问过两次
  const { volumes } = harness(async (node_id) => offer(node_id === 1 ? 1 : 2));
  await volumes.consider(1);
  volumes.snooze();
  await volumes.consider(1);
  volumes.snooze();
  await volumes.consider(1);
  assert.equal(volumes.offer.value, null, "第一卷不再提");

  await volumes.consider(2);
  assert.equal(volumes.offer.value?.container, 2, "换成第二卷：重新开始提");
});

test("切章很快：慢回来的旧答复不许盖掉新状态", async () => {
  const gate: { open: (() => void) | null } = { open: null };
  const { volumes } = harness(async (node_id) => {
    if (node_id === 1) {
      await new Promise<void>((resolve) => {
        gate.open = resolve;
      });
      return offer(5);
    }
    return offer(6, 30, "on_target");
  });

  const slow = volumes.consider(1);
  await volumes.consider(2);
  assert.equal(volumes.offer.value?.container, 6, "新的答复先到就先算数");
  gate.open?.();
  await slow;
  assert.equal(volumes.offer.value?.container, 6, "旧答复回来时只该被丢掉");
});

test("没有提议时按「在这里收卷」什么都不做", async () => {
  const { volumes, closeCalls } = harness(async () => null);
  await volumes.consider(42);
  assert.equal(volumes.offer.value, null);
  await volumes.closeHere();
  assert.deepEqual(closeCalls, [], "没有收卷点就不该碰结构");
});

test("撤销：拆掉刚收好的那一卷，撤销入口随之收走", async () => {
  const { volumes, dissolved, refreshes } = harness();
  await volumes.consider(42);
  await volumes.closeHere();
  await volumes.undo();
  assert.deepEqual(dissolved, [9]);
  assert.equal(volumes.closed.value, null, "撤销之后不再显示撤销入口");
  assert.equal(refreshes(), 2, "撤销同样要重拉目录");
});

test("换作品：提示、撤销入口与'问过几次'的记忆一起清掉", async () => {
  const { volumes } = harness();
  await volumes.consider(42);
  volumes.snooze();
  await volumes.consider(43);
  volumes.snooze();
  await volumes.consider(44);
  assert.equal(volumes.offer.value, null);

  volumes.reset();
  assert.equal(volumes.closed.value, null);
  await volumes.consider(45);
  assert.equal(volumes.offer.value?.count, 24, "新书里重新开始提（旧书那两次「再等等」不带过去）");
});
