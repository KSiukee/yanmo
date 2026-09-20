// 流程（SOP）：作者**自己的写作流程**——"先干什么后干什么"这件事本身也是一件资产。
//
// 这一层只做"取值 ↔ 参数"的转换：两层继承（书的覆盖 → 全局 → 内置那套起手流程）、
// 校验、同事务留痕全在核心。
//
// 三条口径与核心一致（也是这个功能的铁律）：
// - **绝不强制顺序**：可以跳步 / 回退 / 并行，界面上没有"下一步"闸门；
// - **不做打卡**：勾没勾只是作者自己的记号，不统计、不提醒、不惩罚；
// - **核心零文案**：名字 / 说明 / 检查项文字没改过就是 null，界面按 `sop.step.<id>.*` 取字典默认。

import { call, COMMANDS } from "./core";

/** 一个检查项：作者自己写的一句话 + 勾没勾（首版就是纯用户数据）。 */
export interface SopCheck {
  /** 稳定 id（字典键与"勾过没有"都靠它） */
  id: string;
  /** 作者改过的文字；null = 没改过，取字典默认 */
  text: string | null;
  done: boolean;
}

/** 一步可以绑一个动作：这一版只认"打开哪块面板"（`aside`）。 */
export interface SopAction {
  target: string;
  /** `aside` 用第二栏分区的稳定码（`flow` / `creator` / `sop`） */
  value: string;
}

/** 一步：id 是稳定身份，名字 / 说明 / 检查项 / 绑定动作 / 跳过标记都跟着它走。 */
export interface SopStep {
  id: string;
  name: string | null;
  note: string | null;
  /** 参考时长（分钟）——**只是参考值，不作达标判据** */
  minutes: number | null;
  checks: SopCheck[];
  action: SopAction | null;
  /** 这一篇先不走这一步（**不是删除**） */
  skipped: boolean;
}

/** 写回去时给的那一份（整份替换）。 */
export interface Sop {
  steps: SopStep[];
}

/** 这份流程是从哪一层来的——界面据此说清"这份只属于这一篇"。 */
export type SopSource = "default" | "global" | "work";

/** 读出来给人用的那份：步骤 + 它是哪一层来的。 */
export interface ResolvedSop {
  source: SopSource;
  steps: SopStep[];
}

/**
 * 读这一层的流程。
 *
 * `work_id` 给 null = "我平时的写法"（全局那份）；给某本书先看它有没有单独设过。
 */
export const sopRead = (work_id: number | null = null) =>
  call<ResolvedSop>(COMMANDS.sopRead, { work_id });

/**
 * 写这一层的流程：**整份替换**（空清单 = 清掉这一层，回到继承 / 内置）。
 *
 * 写完回读：界面拿到的永远是库里那份（校验夹过、空步删掉都在核心做了）。
 */
export const sopWrite = (work_id: number | null, sop: Sop) =>
  call<ResolvedSop>(COMMANDS.sopWrite, { work_id, sop });

/** 清掉这一层的流程（书的覆盖则是"回到继承全局"）。 */
export const sopReset = (work_id: number | null = null) =>
  call<ResolvedSop>(COMMANDS.sopReset, { work_id });
