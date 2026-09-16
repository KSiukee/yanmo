// 界面侧的错误形状：**核心只给码 + 参数，句子在界面字典里渲染**。
//
// 单独一个文件是为了能**脱离 Tauri 单测**（`api/core.ts` 一 import 就跑不进 Node）——
// "核心报错长什么样、界面怎么渲染"这条链路必须能测。
import { t } from "../locales/index.ts";

/**
 * 核心不可达：浏览器预览模式，或核心启动失败。
 *
 * 它也有 `code`：界面按码做分支的那条路（`error.code`）不该因为"失败得比较早"
 * 就要求调用方分两种写法——两种失败对界面是同一件事：**拿一个码去显示**。
 */
export class CoreUnavailableError extends Error {
  readonly code = "ipc.unavailable";
}

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
 * 去掉**落单的代理项**（半个字符）。
 *
 * JS 的字符串允许只留一半代理项（`"\uD83D"`），Rust 那边的 JSON 解析器**直接拒收**，
 * 而且只回一句英文解析错误——用户看到的就成了"读不出来"。半个字符本来也构不成一个字，
 * 丢掉它让整段文字照常存下去，比让这一次操作整个失败合理。
 */
export function dropLoneSurrogates(text: string): string {
  return text.replace(
    /[\uD800-\uDBFF](?![\uDC00-\uDFFF])|(?<![\uD800-\uDBFF])[\uDC00-\uDFFF]/g,
    "",
  );
}

/**
 * 参数里有没有混进 Vue 的 ref（`{ __v_isRef: true }`）——**忘了 `.value` 的典型症状**。
 *
 * 为什么要专门认它：`.vue` 的 `<script setup>` 段**没人做类型检查**（纯 `tsc` 进不去 SFC），
 * 所以把会话里的 ref 直接当值递出去，编译器一声不吭；到了 IPC 那一层只会回一句
 * 英文反序列化错误，作者与开发者都得猜半天（0.68.1 的体检面板就是这么坏的）。
 * 这里把它翻译成一句能直接照着改的话。
 */
export function findRefArgs(value: unknown, path = "", depth = 0): string | null {
  if (value === null || typeof value !== "object" || depth > 8) return null;
  // 最外层自己就是 ref 时给空串（调用方据此换一句话说）
  if ((value as { __v_isRef?: unknown }).__v_isRef === true) return path;
  for (const [key, item] of Object.entries(value as Record<string, unknown>)) {
    const found = findRefArgs(item, path ? `${path}.${key}` : key, depth + 1);
    if (found !== null) return found;
  }
  return null;
}

/** 递归修掉参数里的半个字符（只走数组与**朴素对象**，别把别的对象克隆坏了）。 */
export function healText<T>(value: T, depth = 0): T {
  if (typeof value === "string") {
    return dropLoneSurrogates(value) as unknown as T;
  }
  if (typeof value !== "object" || value === null || depth > 8) {
    return value;
  }
  if (Array.isArray(value)) {
    return value.map((item) => healText(item, depth + 1)) as unknown as T;
  }
  const proto = Object.getPrototypeOf(value);
  if (proto !== Object.prototype && proto !== null) {
    return value;
  }
  const out: Record<string, unknown> = {};
  for (const [key, item] of Object.entries(value as Record<string, unknown>)) {
    out[key] = healText(item, depth + 1);
  }
  return out as unknown as T;
}

/**
 * 把壳抛回来的东西整成界面认得的错误。
 *
 * Rust 侧回的是 `{ code, params, detail }`；其余（核心根本没起来、Tauri 自己的报错）
 * 一律按"核心不可达"处理——**两者对用户是两回事**：前者是"这件事做不成"，
 * 后者是"程序出了问题"，处置方式不一样。
 */
export function asError(e: unknown): CoreError | CoreUnavailableError {
  // 已经是界面自己整好的错误：**原样交回去**。
  //
  // 再包一层会把 `CoreUnavailableError` 的码（`ipc.unavailable`）当成核心给的码去查字典，
  // 于是屏幕上只剩一个查不到的 `error.ipc.unavailable`，而**真正的原因被这层包装吃掉了**
  // （0.68.1 真机就是这样：体检面板一直报错，报的却是个说不出所以然的键）。
  if (e instanceof CoreError || e instanceof CoreUnavailableError) return e;
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
  // 认不出来的失败：**也别把英文解析错误原样丢到界面上**。
  // 原始文本留一小截在句子里（排查要用），但句子本身是中文的。
  const raw = (typeof e === "string" ? e : String(e)).slice(0, 120);
  return new CoreUnavailableError(t("ipc.unparsable", { detail: raw }));
}
