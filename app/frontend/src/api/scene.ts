// 场景卡：**四格**（视角 / 目标 / 冲突 / 结果）的读写。
//
// 场景卡本身就是目录树上一个节点（名字与正文照旧），这里只管它结构化的那四格。
// 两个入口共用这一份口径：**编辑器里**（打开那张卡时顺手填）与**大纲面板里**
// （一眼看全书哪几张还没填）。

import { call, COMMANDS, type SceneFields } from "./core";

export type { SceneFields };

/** 一张场景卡给界面看的形状。 */
export interface SceneCard {
  node_id: number;
  /** 树上那个名字（可能空着——界面说"还没起名"） */
  title: string;
  fields: SceneFields;
}

/** 这本书里所有场景卡（按树里的顺序），带上各自的四格。 */
export const sceneList = (work_id: number) =>
  call<SceneCard[]>(COMMANDS.sceneList, { work_id });

/** 存一张场景卡的四格（整行覆盖），回执是**库里真有的那一份**。 */
export const sceneFieldsSave = (fields: SceneFields) =>
  call<SceneFields>(COMMANDS.saveSceneFields, { ...fields });
