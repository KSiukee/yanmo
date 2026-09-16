// 设定卡（人物 / 设定）：大纲冲突检测的数据源之一。
//
// 这一层只做"取值 ↔ 参数"的转换；整卡覆盖、空项丢掉、软删全在核心。
//
// 两个口径（与核心一致）：
// - **整卡覆盖**：表单里是什么样，交上去就是什么样（不做稀疏补丁）；
// - **只有键没值的那条留着**——那是"还没填"，不是手滑。

import { call, COMMANDS } from "./core";

/** 哪一类设定卡（与核心 `EntityKind` 的稳定码一致）。 */
export type EntityKind = "person" | "setting";

/** 一条设定：键 + 值（都是作者自己写的字）。 */
export interface Attribute {
  key: string;
  value: string;
}

/** 一张设定卡。 */
export interface EntityCard {
  id: number;
  work_id: number;
  kind: EntityKind;
  name: string;
  /** 别的叫法：字 / 号 / 绰号 / 小名 */
  aliases: string[];
  attributes: Attribute[];
  note: string;
  created_at: number;
  updated_at: number;
}

/** 一整屏：这本书的设定卡 + 按类型分的数字（面板上那两个筛选项）。 */
export interface EntityBoard {
  cards: EntityCard[];
  persons: number;
  settings: number;
}

/** 交上去的一份卡（新建与修改共用：**整卡覆盖**）。 */
export interface EntityCardForm {
  kind: string;
  name: string;
  aliases: string[];
  attributes: Attribute[];
  note: string;
}

/** 列这本书的设定卡（人物 / 设定一起给，按名字排）。 */
export const entityList = (work_id: number) =>
  call<EntityBoard>(COMMANDS.entityList, { work_id });

/** 新建一张（名字空着会被核心当场拒）。 */
export const entityCreate = (work_id: number, form: EntityCardForm) =>
  call<EntityBoard>(COMMANDS.entityCreate, { work_id, form });

/** 改一张（整卡覆盖）。 */
export const entityUpdate = (id: number, form: EntityCardForm) =>
  call<EntityBoard>(COMMANDS.entityUpdate, { id, form });

/** 删一张（软删：库里还留着，只是不列出来了）。 */
export const entityDelete = (id: number) => call<EntityBoard>(COMMANDS.entityDelete, { id });
