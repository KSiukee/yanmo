// 诊断的渲染入口：核心只给「码 + 参数」，句子在这里按码查字典。
//
// 为什么要一个单独的入口：诊断有两条来路——新记录是对象（码 + 参数），
// 老账本里落盘的是句子。**两种都要能显示**，不能因为改格式让作者看见空白
// （账本是库之外的 JSON，改格式最容易在这一处伤到人）。

import type { Diagnosis, Reason } from "../api/core/backup";
import { t } from "./index";

/** 把一条「原因」渲染成人话：诊断按码查字典，老句子原样返回。 */
export function describeReason(reason: Reason | null | undefined): string {
  if (!reason) return "";
  if (typeof reason === "string") return reason;
  return describeDiagnosis(reason);
}

/** 把一条诊断渲染成人话（字典里没有这个码时 `t` 会原样返回键名，一眼看得见）。 */
export function describeDiagnosis(diagnosis: Diagnosis): string {
  return t(`error.${diagnosis.code}`, diagnosis.params ?? {});
}
