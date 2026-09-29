// 界面卡顿度量的验收：**漂移是"这个界面卡没卡"的唯一读数**，
// 算错的方向有两个，都要钉死——
//
// - 把零头凑成假卡顿（作者一晚上看见几十条"卡了"），
// - 或者把真卡住的那一次漏掉、或一次卡顿记成好几回（那读数就没法用了）。

import { test } from "node:test";
import assert from "node:assert/strict";

import { DriftWatch, STALL_MS, TICK_MS, secondsText } from "./drift.ts";

/** 按拍喂：第 `n` 拍的时刻是 `n * TICK_MS`（准时）。 */
function feed(watch: DriftWatch, beats: number): number[] {
  const lags: number[] = [];
  for (let beat = 0; beat < beats; beat += 1) {
    lags.push(watch.tick(beat * TICK_MS));
  }
  return lags;
}

test("准时的拍子一条都不记——第一拍只是对齐", () => {
  const watch = new DriftWatch();
  const lags = feed(watch, 10);
  assert.deepEqual(lags, [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
  assert.deepEqual(watch.read(), { count: 0, maxMs: 0, totalMs: 0, last: null });
  assert.deepEqual(watch.stalls(), []);
});

test("迟到整整一拍才算卡：差一毫秒都不算", () => {
  const watch = new DriftWatch();
  feed(watch, 1);
  assert.equal(watch.tick(TICK_MS + (STALL_MS - 1)), 0, "差一毫秒：不算");
  assert.equal(watch.read().count, 0);

  // 从他给的那一拍重新算：整整一拍迟到，记下来
  assert.equal(watch.tick(TICK_MS * 2 + STALL_MS - 1 + STALL_MS), STALL_MS);
  assert.equal(watch.read().count, 1);
});

test("零头不累积——连着几拍都差一点，不该凑成一场假卡顿", () => {
  const watch = new DriftWatch();
  feed(watch, 1);
  // 每一拍都迟到 900ms（不到一拍），喂 20 拍
  for (let beat = 1; beat <= 20; beat += 1) {
    assert.equal(watch.tick(beat * TICK_MS + 900), 0, `第 ${beat} 拍`);
  }
  assert.equal(watch.read().count, 0, "零头攒起来不是卡顿：900ms × 20 也一笔记都不该有");
});

test("卡了 3 秒只报一次：那一刻之后重新对齐", () => {
  const watch = new DriftWatch();
  feed(watch, 1);
  // 主线程被占住 3 秒：本该第 1 拍到的回调，到第 1 拍 +3 秒才跑
  const lag = watch.tick(TICK_MS + 3_000);
  assert.equal(lag, 3_000);
  assert.equal(watch.read().count, 1, "卡一回记一回");

  // 后面几拍都是准时的（重新对齐，不会连着再报几次）
  for (let beat = 2; beat <= 5; beat += 1) {
    assert.equal(watch.tick(beat * TICK_MS + 3_000), 0, `第 ${beat} 拍`);
  }
  assert.equal(watch.read().count, 1, "一回卡顿只算一回");
});

test("统计口径：几次、最久多久、加起来多久、最近一次", () => {
  const watch = new DriftWatch();
  feed(watch, 1);
  watch.tick(TICK_MS + 2_000); // 2 秒
  watch.tick(TICK_MS + 2_000 + TICK_MS); // 准时
  watch.tick(TICK_MS * 3 + 2_000 + 5_000); // 再卡 5 秒

  const stats = watch.read();
  assert.equal(stats.count, 2);
  assert.equal(stats.maxMs, 5_000, "最久那次");
  assert.equal(stats.totalMs, 7_000, "加起来");
  assert.deepEqual(stats.last, { at: TICK_MS * 3 + 7_000, ms: 5_000 }, "最近一次是最后那回");
});

test("提前触发（定时器早了一点）不算卡顿，也不该把账算歪", () => {
  const watch = new DriftWatch();
  feed(watch, 1);
  assert.equal(watch.tick(TICK_MS - 30), 0, "早 30 毫秒：不算");
  assert.equal(watch.tick(TICK_MS * 2 - 60), 0);
  assert.equal(watch.read().count, 0);
});

test("窗口从后台回来：reset 之后的第一拍只对齐", () => {
  const watch = new DriftWatch();
  feed(watch, 1);
  watch.reset();
  // 后台待了十分钟（浏览器把定时器压住了），回来第一拍：**这不是卡顿**
  assert.equal(watch.tick(TICK_MS + 600_000), 0);
  assert.equal(watch.read().count, 0);
  // 回来之后照旧量
  assert.equal(watch.tick(TICK_MS * 2 + 600_000 + 4_000), 4_000);
  assert.equal(watch.read().count, 1);
});

test("留档有界，且新的在前（面板从头看起）", () => {
  const watch = new DriftWatch();
  feed(watch, 1);
  let at = TICK_MS;
  for (let round = 0; round < 14; round += 1) {
    at += TICK_MS + STALL_MS; // 这一拍迟到整整一拍：卡一回
    watch.tick(at);
    at += TICK_MS; // 下一拍准时
    watch.tick(at);
  }
  const stalls = watch.stalls();
  assert.equal(watch.read().count, 14, "卡几回就该记几回");
  assert.equal(stalls.length, 10, "面板只看最近十条（有界）");
  assert.ok(stalls[0].at > stalls[stalls.length - 1].at, "新的在前");
});

test("秒数写在界面上：整数不拖 `.0`，零头保留一位", () => {
  assert.equal(secondsText(1_000), "1");
  assert.equal(secondsText(3_200), "3.2");
  assert.equal(secondsText(12_000), "12");
  assert.equal(secondsText(360), "0.4", "不足一秒也照实说（不出 0——那看着像没卡）");
});
