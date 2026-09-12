// 界面侧的错误形状：**核心只给码 + 参数，句子在界面字典里渲染**。
//
// 单独一个文件是为了能**脱离 Tauri 单测**（`api/core.ts` 一 import 就跑不进 Node）——
// "核心报错长什么样、界面怎么渲染"这条链路必须能测。
import { t } from "../locales/index.ts";

/** 核心不可达：浏览器预览模式，或核心启动失败。 */
export class CoreUnavailableError extends Error {}

/**
 * 核心 / 壳明确报回来的失败：**码 + 参数**。
 *
 * `message` 是查字典渲染好的句子（`error.<码>`），所以界面照常读 `error.message` 就行；
 * 想按码做分支（比如"这一章已经没了"换个说法）时用 `code`。
 */
export class CoreError extends Error {
  readonly code: string;
  readonly params: Record<string, string>;

  constructor(code: string, params: Record<string, string>) {
    super(t(`error.${code}`, params));
    this.name = "CoreError";
    this.code = code;
    this.params = params;
  }
}

/**
 * 把壳抛回来的东西整成界面认得的错误。
 *
 * Rust 侧回的是 `{ code, params, detail }`；其余（核心根本没起来、Tauri 自己的报错）
 * 一律按"核心不可达"处理——**两者对用户是两回事**：前者是"这件事做不成"，
 * 后者是"程序出了问题"，处置方式不一样。
 */
export function asError(e: unknown): Error {
  if (e !== null && typeof e === "object" && typeof (e as { code?: unknown }).code === "string") {
    const raw = (e as { code: string; params?: unknown }).params;
    const params: Record<string, string> = {};
    if (raw !== null && typeof raw === "object") {
      for (const [name, value] of Object.entries(raw as Record<string, unknown>)) {
        params[name] = String(value);
      }
    }
    return new CoreError((e as { code: string }).code, params);
  }
  return new CoreUnavailableError(typeof e === "string" ? e : String(e));
}
