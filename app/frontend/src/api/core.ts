// 壳与核心之间**唯一的通道**（拆成多个文件，见同目录 `core/`）。
//
// 纪律（不是口头约定，有机械检查盯着）：
// - 只有 `api/` 下的文件可以 import "@tauri-apps/api"；
// - 组件一律从这里取数据，不直接 invoke；
// - 这里不出现文件系统 API，也没有「路径参数」——数据在哪由 Rust 侧决定。
//
// 这个文件只是**转出口**（facade）：真正的实现按域放在 `core/` 下，导出面与拆之前一模一样。

export { call, COMMANDS, CoreError, CoreUnavailableError } from "./core/transport";
export * from "./core/system";
export * from "./core/editor";
export * from "./core/tree";
export * from "./core/work";
export * from "./core/snapshot";
export * from "./core/backup";
export * from "./core/appearance";
export * from "./core/writing";
export * from "./core/typeset";
export * from "./core/compile";
export * from "./core/import";
