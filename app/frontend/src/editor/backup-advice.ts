// 备份目标的「该用哪块盘」——纯逻辑（不碰界面、不碰命令），可脱开界面单测。
//
// 为什么需要它：备份面板一直只是把可见的盘**平铺列出来**，让作者自己挑；
// 没插盘时也只有一句「插个盘吧」。推荐的规则其实很清楚，写死在这里比让作者猜好：
//
// 1. **跳过数据所在的那块盘**——备份与稿库在同一块盘上，盘坏了两份一起没；
// 2. **剩余空间太小的不推荐**——放不下几份备份，推荐了反而添乱；
// 3. **可移动盘优先**——备份完能拔走、放到别处，这正是"异盘备份"的意义；
// 4. 同类里**剩余空间大的优先**。
//
// 已经配成目标的盘由调用方先滤掉（本函数只对"候选盘"做推荐）。

import type { BackupVolume } from "../api/core/backup";

/** 剩余空间下限：低于这个数的盘不推荐（当前 512 MB）。 */
export const MIN_FREE_BYTES = 512 * 1024 * 1024;

/** 推荐结果：哪块盘 + 为什么是它（界面按 reason 挑一句文案）。 */
export interface VolumeAdvice {
  volume: BackupVolume;
  reason: "removable" | "roomy";
}

/** 从候选盘里挑一块最该用来备份的；没有合适的就说没有（`null`）。 */
export function recommendVolume(volumes: BackupVolume[]): VolumeAdvice | null {
  const candidates = volumes.filter(
    (volume) => !volume.holds_data && volume.free_bytes >= MIN_FREE_BYTES,
  );
  if (candidates.length === 0) return null;
  const sorted = [...candidates].sort((a, b) => {
    if (a.removable !== b.removable) return a.removable ? -1 : 1;
    return b.free_bytes - a.free_bytes;
  });
  const best = sorted[0];
  return { volume: best, reason: best.removable ? "removable" : "roomy" };
}
