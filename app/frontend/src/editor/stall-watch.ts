// 卡顿度量的**接线**：每秒量一次、把读数摆给界面看、迟到整整一拍的顺手递给壳。
//
// 为什么与 [`./drift.ts`]（纯逻辑那一半）分开：那一半要能在 Node 里跑单测，
// 所以它一个 import 都没有；这里要碰定时器、碰 DOM、碰壳的命令，天然不可单测——
// 分开之后，"算得对不对"（drift）与"接线接没接对"（本文件）各归各的。
//
// 模块级单例（同 [`./diagnose.ts`] 的写法）：量的是"**这个界面进程**卡没卡"，
// 与哪本书、哪一屏都无关——挂在编辑器里会随切章重建，数字就断了。

import { ref, type Ref } from "vue";

import { reportUiLag } from "../api/perf";
import { DriftWatch, TICK_MS, type DriftStats } from "./drift";

/** 一次都没卡过（界面上的空态与初值）。 */
const NO_STALLS: DriftStats = { count: 0, maxMs: 0, totalMs: 0, last: null };

/** 本次会话的卡顿读数（组件直接读它；换页面不丢）。 */
export const stallStats: Ref<DriftStats> = ref(NO_STALLS);

const watch = new DriftWatch();
let timer: number | null = null;

/** 一拍：量一次、记一次，迟到整整一拍的顺手递给壳（进诊断日志与库里那本账）。 */
function beat(): void {
  if (globalThis.document?.visibilityState !== "visible") {
    // 后台/被挂起：这不是卡顿（浏览器在省电），重新对齐等回来再量
    watch.reset();
    return;
  }
  const lag = watch.tick(performance.now());
  // 递给壳只是"留痕"（日志一句 + 库里一条），失败不影响任何事——所以直接吞掉
  if (lag > 0) void reportUiLag(lag).catch(() => undefined);
  stallStats.value = watch.read();
}

/**
 * 开始量（界面挂上时调**一次**）。返回停止函数。
 *
 * 重复调用是安全的：已经在量就直接返回（热重载/重挂别叠出第二个定时器）。
 */
export function watchForStalls(target: Document = document): () => void {
  if (timer !== null) return () => undefined;

  timer = window.setInterval(beat, TICK_MS);
  const onVisibility = () => {
    // 进出后台都重新对齐：那一段时间不管量出什么都与界面卡不卡无关
    watch.reset();
    stallStats.value = watch.read();
  };
  target.addEventListener("visibilitychange", onVisibility);

  return () => {
    if (timer !== null) window.clearInterval(timer);
    timer = null;
    target.removeEventListener("visibilitychange", onVisibility);
  };
}
