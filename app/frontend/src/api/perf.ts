// 慢命令与界面卡顿那几条命令的**类型与调用**（命令名仍在 `./core` 的白名单里统一登记）。
//
// 为什么单开一个文件：`core.ts` 是"唯一能 import Tauri API 的入口"，它贴着行数棘轮的上限
// （仓库里那条规矩：已登记的超限文件只许减不许增）。新域的口径同 `volumes.ts` / `mirror.ts`：
// **命令名进白名单（`core.ts`），类型与薄封装放各自域的文件里**——通道仍然只有一条。

import { call, COMMANDS } from "./core";

/** 一条命令的耗时（单位毫秒）。 */
export interface SlowCommandReadout {
  command: string;
  ms: number;
}

/** 库里记着的那一条（界面被重载过之后，壳里那份内存记录就没了，只有它还在）。 */
export interface PersistedSlowCommand {
  command: string;
  ms: number;
  /** 记下那一刻（unix 毫秒） */
  at: number;
}

/**
 * 慢命令的一份读数：跑完的、**此刻还在飞的**、以及库里那一条。
 *
 * "还在飞"是给"界面好像没反应"那一会儿看的：正卡在哪条命令上一眼就知道。
 * 判"多久算久"的机器在壳里（5 秒），这里只是读数。
 */
export interface SlowReport {
  recent: SlowCommandReadout[];
  inflight: SlowCommandReadout[];
  last_in_db: PersistedSlowCommand | null;
}

/** 读一份慢命令读数（设置里那块"运行诊断"用它）。 */
export const readSlowReport = () => call<SlowReport>(COMMANDS.slowReport);

/**
 * 报一次界面卡顿（1 秒定时器迟了多久）。
 *
 * 壳收到只做两件事：诊断日志里记一句、库里留一条——**什么都不改**。
 * 所以这里失败也不影响任何事（调用方直接吞掉错误）。
 */
export const reportUiLag = (ms: number) => call<void>(COMMANDS.uiLag, { ms });
