// 每日码字目标：**全局打底 + 每书可覆盖**。
//
// 单独一个模块，是因为它和"日历"的变化理由不一样：
// - 目标是**偏好**（存在 settings 里，跟人不跟数据，核心见 `store::appearance`）；
// - 日历是**账本**（`writing_days` 一天一行，核心见 `store::writing`）。
// 两件事凑在一个文件里，改目标的手感迟早会牵动日历的取数。
//
// 只认接口不认具体命令（真命令在会话层注入）：可以脱离界面与核心单测。

import { ref, type Ref } from "vue";

import type { Appearance, AppearancePatch } from "../api/core";

/** 设目标时写哪一层：只设这本书（覆盖） / 设为所有作品的默认。 */
export type GoalTarget = "work" | "default";

/** 目标要用的两个动作（与设置面板同一份存储）。 */
export interface GoalTransport {
  read: (work_id: number | null) => Promise<Appearance>;
  write: (work_id: number | null, patch: AppearancePatch) => Promise<Appearance>;
}

export interface DailyGoal {
  /** 现在生效的那一份（本书覆盖优先；null = 没设） */
  goal: Ref<number | null>;
  /** 全局默认（面板里说明"其他作品照这个来"用） */
  defaultGoal: Ref<number | null>;
  /** 跟当前作品重新读一次（打开章节、换书、设完目标后） */
  load: () => Promise<void>;
  /** 设目标：`value` 传 0（或 null）＝清掉那一层 */
  set: (value: number | null, target: GoalTarget) => Promise<void>;
}

export function useDailyGoal(transport: GoalTransport, workId: Ref<number | null>): DailyGoal {
  const goal = ref<number | null>(null);
  const defaultGoal = ref<number | null>(null);

  async function load(): Promise<void> {
    const work_id = workId.value;
    // 全局那份永远读：既当默认，也是没有本书覆盖时生效的那一份
    const global = await transport.read(null);
    defaultGoal.value = global.daily_goal;
    if (work_id === null) {
      goal.value = global.daily_goal;
      return;
    }
    // 这本书生效的那一份（核心已经替界面把"覆盖 → 全局 → 默认"合好了）
    goal.value = (await transport.read(work_id)).daily_goal;
  }

  return {
    goal,
    defaultGoal,
    load,
    set: async (value, target) => {
      const target_work = target === "work" ? workId.value : null;
      if (target === "work" && target_work === null) return;
      // 传 0 就是"清掉这一层"（与核心同一条规矩：0 / 负数当没设目标）
      await transport.write(target_work, { daily_goal: value ?? 0 });
      await load();
    },
  };
}
