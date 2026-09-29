// 界面卡顿度量：**定时器漂移**。
//
// 为什么用这个办法：界面卡住时（长任务、布局爆炸、WebView 忙别的）最先显形的是定时器——
// 一个 1 秒的 `setInterval` 会迟到。它迟到多久，主线程就被占住多久，而且**不需要任何采样库**。
//
// 三条分寸：
//
// 1. **迟到整整一拍才算卡**（[`STALL_MS`]）：一秒以内的抖动是浏览器调度的正常噪音，
//    记下来只会把报告淹掉，让真正的卡顿看不见；
// 2. **每一拍都重新对齐**：卡了 3 秒只报**一次** "3 秒"，不会连着几拍各报一次
//    （那种账看着像卡了好几回，其实只卡了一回）；
// 3. **只量可见的时候**：窗口在后台时浏览器会把定时器压到几分钟一次，那不是卡顿，
//    是省电——切到后台/切回来都重新对齐。
// 界面文案不在这儿：这一层只产数字（`{ms}` 这类读数由组件从字典取字）。
//
// **这一份不 import 任何东西**（连 vue 都不引）：它是纯逻辑，单测直接把它搬进 Node 跑；
// 接上定时器与壳那几条命令的是另一件（[`./stall-watch.ts`]）。

/** 一拍多长：1 秒。 */
export const TICK_MS = 1_000;
/** 迟到多少算"卡了"：**整整一拍**（见文件头第 1 条）。 */
export const STALL_MS = TICK_MS;
/** 留多少条给面板看（有界：这是读数，不是账本）。 */
const KEEP = 10;

/** 卡过的一次：迟了多久、是哪一拍的读数（单调毫秒，给排序与判重用的，不是墙上时间）。 */
export interface Stall {
  at: number;
  ms: number;
}

/** 本次会话的卡顿读数（界面上那句"刚才卡了 N 秒"就是它）。 */
export interface DriftStats {
  /** 一共卡过几次 */
  count: number;
  /** 最久那一次卡了多久 */
  maxMs: number;
  /** 加起来卡了多久 */
  totalMs: number;
  /** 最近一次（没卡过就是 null） */
  last: Stall | null;
}

/**
 * 漂移计：喂它每一拍的读数，它告诉你哪几拍迟到了。
 *
 * 纯逻辑（不看表、不碰 DOM、不落盘）：单测拿一串数字把时间拨快就能测死。
 */
export class DriftWatch {
  /** 上一拍的实际读数（null = 还没对齐） */
  private previous: number | null = null;
  private count = 0;
  private max = 0;
  private total = 0;
  private latest: Stall | null = null;
  private history: Stall[] = [];

  /**
   * 一拍：返回这一次**迟到多少毫秒**（没到整整一拍就算准时，返回 0）。
   *
   * 无论迟没迟，下一拍都从这一次重新算——零头不累积（累积起来的零头会凑出一场假卡顿）。
   */
  tick(now: number): number {
    const previous = this.previous;
    this.previous = now;
    if (previous === null) return 0; // 第一拍只对齐

    const lag = now - previous - TICK_MS;
    if (lag < STALL_MS) return 0; // 准时（含提前触发与零头）

    this.count += 1;
    this.total += lag;
    this.max = Math.max(this.max, lag);
    const stall: Stall = { at: now, ms: lag };
    this.latest = stall;
    this.history.push(stall);
    while (this.history.length > KEEP) this.history.shift();
    return lag;
  }

  /**
   * 重新对齐：**这一段不算卡顿**。
   *
   * 用在窗口从后台回来（或切到后台）——那会儿定时器被浏览器压着，迟到的不是界面。
   */
  reset(): void {
    this.previous = null;
  }

  /** 现在读到什么（界面上那句读数的唯一来源）。 */
  read(): DriftStats {
    return { count: this.count, maxMs: this.max, totalMs: this.total, last: this.latest };
  }

  /** 卡过的这几回（**新的在前**，最多 [`KEEP`] 条）。 */
  stalls(): Stall[] {
    return [...this.history].reverse();
  }
}

/**
 * 毫秒 → 给人看的秒数（**只给数字，不给单位**：单位那一半是界面文案，住字典里）。
 *
 * 规则：整秒就写整数（`1` 而不是 `1.0`——"卡了 1.0 秒"读着别扭），
 * 不足一秒或带零头保留一位小数（`3.2`）。读数本来就是粗的，不必装成精确到毫秒。
 */
export function secondsText(ms: number): string {
  const seconds = ms / 1_000;
  return Number.isInteger(seconds) ? String(seconds) : seconds.toFixed(1);
}

/**
 * 送给壳的毫秒数：**必须是整数**。
 *
 * 真机走查逮到过：漂移是 `performance.now()` 相减出来的**小数**（如 2260.3999…），
 * 直接递给壳的话，那边的 `u64` 反序列化当场失败——而这条上报是"有则更好"的账，
 * 失败被吞掉，于是界面上有读数、库里一条都没有（最难查的那种"看起来在工作"）。
 * 非有限值（NaN / Infinity，理论上不该出现）一律按 0 处理：**别把坏数递给壳**。
 */
export function reportMs(lag: number): number {
  if (!Number.isFinite(lag)) return 0;
  return Math.max(0, Math.round(lag));
}
