// 界面级验收：只在 `--self-test-ui` 下有效。
//
// 两件事分开：① 探测"这次是不是验收模式"（正常启动回 false，别的一律不做）；
// ② 把界面自测的几条数字交回壳（壳并进同一份报告）。
// 口径由交上去的每一条 `note` 写清——与命令行入口的数字**不混在一个键里**。

import { call, COMMANDS } from "./transport";

/** 界面自测交回去的一步（名字 / 毫秒 / 附注；口径写在 note 里）。 */
export interface ProbeStep {
  name: string;
  ms: number;
  note: string;
}

/** 现在是不是验收模式（正常启动是 `false`）。 */
export const acceptanceProbeActive = () => call<boolean>(COMMANDS.acceptanceProbeActive);

/** 把界面自测的结果交给壳（只在验收模式收，且只收第一份）。 */
export const acceptanceUiProbe = (steps: ProbeStep[]) =>
  call<boolean>(COMMANDS.acceptanceUiProbe, { steps });
