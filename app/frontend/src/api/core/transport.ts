// 唯一碰 Tauri 的地方：invoke / listen / 窗口与版本，以及**唯一一处**错误渲染。

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { asError, CoreError, CoreUnavailableError, findRefArgs, healText } from "../errors";
import { COMMANDS } from "../gateway";
import { t } from "../../locales/index.ts";

export { COMMANDS, CoreError, CoreUnavailableError };
// 给同目录其它文件用的底层件（不从这里对界面公开的部分，facade 不会转出去）
export { listen, getVersion, getCurrentWindow, asError };
export type { UnlistenFn };

export async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    // 参数里混进 ref（忘了 `.value`）：**当场喊清楚**。放到 IPC 那一层只会回一句
    // 英文反序列化错误，谁都看不出问题在哪儿（见 api/errors.ts 的说明）。
    const refArg = args ? findRefArgs(args) : null;
    if (refArg !== null) {
      throw new Error(
        refArg === ""
          ? t("ipc.ref_arg_root", { command })
          : t("ipc.ref_arg", { arg: refArg, command }),
      );
    }
    // 参数先修一遍「半个字符」：JS 允许落单代理项，Rust 的 JSON 解析器直接拒收，
    // 真让它过去，用户会收到一句英文解析错误（见 api/errors.ts 的说明）。
    return await invoke<T>(command, args ? healText(args) : args);
  } catch (e) {
    throw asError(e);
  }
}
