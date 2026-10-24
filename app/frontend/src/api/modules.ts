// 模块管理页的取值：**卡片 + 四个动作**。规则全在核心；这里只做名字映射。
//
// 纪律：界面不传路径、不碰文件；模块数据在哪由核心决定（卸载默认保留，删数据是另一个
// 显式动作，这一版还没开）。

import { call, COMMANDS } from "./core";

/** 管理页一行（与核心 `ModuleCard` 一一对应）。 */
export interface ModuleCard {
  id: string;
  name: string;
  description: string;
  version: string;
  author: string;
  certified: boolean;
  cert_edition: number;
  certified_at: number;
  enabled: boolean;
  favorite: boolean;
  pinned: boolean;
  reads: string[];
  own_data: boolean;
  network: boolean;
  fingerprint: string;
  source: string;
  installed_at: number;
  updated_at: number;
}

/** 模块声明的能力（与核心 `ModuleCapabilities` 一一对应；授予是它的子集）。 */
export interface ModuleCapabilities {
  read: string[];
  own_data: boolean;
  cross_read: string[];
  ask_questions: boolean;
  network: boolean;
}

/** 导入前的预览：**只给事实**，界面逐项征得同意。 */
export interface ImportPreview {
  module_id: string;
  name: string;
  description: string;
  version: string;
  author: string;
  protocol: number;
  expected_protocol: number;
  compatible: boolean;
  fingerprint: string;
  declared: ModuleCapabilities;
  entry_bytes: number;
  certified: boolean;
  cert_edition: number;
  certified_at: number;
}

export const modulesList = () => call<ModuleCard[]>(COMMANDS.modulesList);

export const moduleSetEnabled = (module_id: string, enabled: boolean) =>
  call<void>(COMMANDS.moduleSetEnabled, { module_id, enabled });

export const moduleSetFavorite = (module_id: string, favorite: boolean) =>
  call<void>(COMMANDS.moduleSetFavorite, { module_id, favorite });

export const moduleSetPinned = (module_id: string, pinned: boolean) =>
  call<void>(COMMANDS.moduleSetPinned, { module_id, pinned });

export const moduleUninstall = (module_id: string) =>
  call<void>(COMMANDS.moduleUninstall, { module_id });

export const moduleImportPick = (title: string) =>
  call<ImportPreview | null>(COMMANDS.moduleImportPick, { title });

export const moduleImportConfirm = (granted: ModuleCapabilities, expected_fingerprint: string) =>
  call<ModuleCard>(COMMANDS.moduleImportConfirm, { granted, expected_fingerprint });

export const moduleImportCancel = () => call<void>(COMMANDS.moduleImportCancel);

/** 跑一次模块的报告（宿主跑完的那一份）。 */
export interface ModuleRunReport {
  exit_code: number | null;
  timed_out: boolean;
  emitted: string[];
  stderr: string;
  ipc_error: string | null;
  job_assigned: boolean;
}

export const moduleRun = (module_id: string) =>
  call<ModuleRunReport>(COMMANDS.moduleRun, { module_id });
