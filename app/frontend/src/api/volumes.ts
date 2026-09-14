// 分卷那几条命令的**类型与调用**（命令名仍在 `./core` 的白名单里统一登记）。
//
// 为什么单开一个文件：`core.ts` 是"唯一能 import Tauri API 的入口"，它已经在上限边上了
// （见 `tools/qa/known_health.json` 的棘轮：只许减不许增）。所以新域的口径是
// **命令名进白名单（`core.ts`），类型与薄封装放各自域的文件里**——
// 通道仍然只有一条（`call` 只在 core.ts 里碰 Tauri），只是把"翻译层"分了家。
//
// 这一层只做"取值 ↔ 参数"的转换：怎么算阈值、怎么动结构，全在核心。

import { call, COMMANDS } from "./core";

/** 一本书的分卷口径：作者设的、从他收好的卷学到的、以及**真正在用的**那个阈值。 */
export interface VolumePlan {
  /** 作者设的「大概几章一卷」（没设过是 null） */
  target: number | null;
  /** 从他收好的卷学到的中位数（样本 ≥2 才有） */
  learned: number | null;
  /** 真正在用的阈值（学到的优先，其次作者设的）；null = 没得提示 */
  effective: number | null;
}

/** 收卷提议：在这个节点（一章）之后可以收卷了。 */
export interface VolumeOffer {
  plan: VolumePlan;
  /** 到这一章为止这一卷已有几章 */
  count: number;
  /** early = 提前 / on_target = 正好 / late = 延后 */
  phase: "early" | "on_target" | "late";
  /** 收卷点落在哪一卷上（null = 章还散在根上）——界面按它记住"这一卷问过了" */
  container: number | null;
}

/** 收卷的结果：新卷是谁、有没有顺手起第一章（界面把光标落过去）。 */
export interface CloseVolumeReceipt {
  volume_id: number;
  opened_chapter: number | null;
  moved: number;
}

/** 撤卷的结果：抬回去几项、并进了哪一卷（null = 抬到父层原位）。 */
export interface DissolveVolumeReceipt {
  moved: number;
  merged_into: number | null;
}

/** 分卷口径（界面显示「本卷 12/30 章」的分母就是 effective）。 */
export const volumePlan = (work_id: number) => call<VolumePlan>(COMMANDS.volumePlan, { work_id });

/** 该不该在这一章之后提一句收卷（null = 不用提）。 */
export const volumeOffer = (node_id: number) =>
  call<VolumeOffer | null>(COMMANDS.volumeOffer, { node_id });

/** 在这里收卷（title 留空 = 按位置渲染成「第 N 卷」）。 */
export const volumeClose = (node_id: number, title: string) =>
  call<CloseVolumeReceipt>(COMMANDS.volumeClose, { node_id, title });

/** 撤卷：取消这一卷的分卷，里面的东西按原顺序还回去。 */
export const volumeDissolve = (volume_id: number) =>
  call<DissolveVolumeReceipt>(COMMANDS.volumeDissolve, { volume_id });
