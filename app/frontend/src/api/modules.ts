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
  enabled: boolean;
  favorite: boolean;
  pinned: boolean;
  reads: string[];
  own_data: boolean;
  network: boolean;
  installed_at: number;
  updated_at: number;
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
