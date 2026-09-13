// 码字统计：**今日进度**与**码字日历**的数据与开关。
//
// 三条分寸：**账本在核心**（`store::writing`），这里只问数、不自己判"哪一天算哪一天"；
// **今日进度跟着落盘走**（会话在每次落盘成功后调一次 `refreshToday()`，不开轮询）；
// **目标走 appearance**（全局 + 每书覆盖，单独一个模块见 `writing-goal.ts`），
// 没设目标就只显示"今天写了多少"，不拿一个假目标糊弄作者。
//
// 只认接口不认具体命令（真命令在会话层注入）：可以脱离界面与核心单测。

import { ref, type Ref } from "vue";

import type { Appearance, AppearancePatch, WritingDay, WritingOverview } from "../api/core";
import { useDailyGoal, type GoalTarget } from "./writing-goal";
import { monthRange, shiftMonth, weekRange } from "./writing-days";

/** 日历看谁的数据：当前这本书 / 全部作品合计。 */
export type WritingScope = "work" | "all";

export type { GoalTarget };

/** 要用到的几个动作（会话层注入真命令，测试注入替身）。 */
export interface WritingTransport {
  today: (work_id: number | null, tz_offset_minutes: number) => Promise<WritingDay>;
  overview: (
    work_id: number | null,
    tz_offset_minutes: number,
    from_day: string,
    to_day: string,
  ) => Promise<WritingOverview>;
  /** 读偏好（拿每日目标；与设置面板同一份存储） */
  readGoal: (work_id: number | null) => Promise<Appearance>;
  writeGoal: (work_id: number | null, patch: AppearancePatch) => Promise<Appearance>;
  /** 本地时区偏移（东八区 = 480）——"今天"由它定 */
  tzOffsetMinutes: () => number;
}

export interface WritingOptions {
  transport: WritingTransport;
  /** 当前作品（null = 还没打开书） */
  workId: Ref<number | null>;
  onError?: (message: string) => void;
}

export interface WritingState {
  /** 今天写了多少（**当前这本书**；状态栏那行小字用）；还没问回来是 null */
  today: Ref<WritingDay | null>;
  /** 现在生效的每日目标（本书覆盖优先；null = 没设） */
  goal: Ref<number | null>;
  /** 全局默认目标（面板里说明"其他作品照这个来"用） */
  defaultGoal: Ref<number | null>;
  /** 日历面板开着没有 */
  visible: Ref<boolean>;
  /** 日历看谁的数据 */
  scope: Ref<WritingScope>;
  /** 正在看的月份 */
  month: Ref<{ year: number; month: number }>;
  /** 区间内**有记录**的日子（没记录的不返回，视图自己补空格） */
  days: Ref<WritingDay[]>;
  /** 连续码字天数 */
  streak: Ref<number>;
  busy: Ref<boolean>;
  /** 落盘成功后 / 打开一章后刷一次今日（不问目标，开销只有一行查询） */
  refreshToday: () => Promise<void>;
  /** 跟当前作品重新读一次目标（打开章节、从设置回来后用） */
  loadGoal: () => Promise<void>;
  /** 开日历面板：重读目标与区间数据 */
  open: () => Promise<void>;
  close: () => void;
  setScope: (scope: WritingScope) => Promise<void>;
  shiftMonth: (delta: number) => Promise<void>;
  /** 设目标：`value` 传 0（或 null）＝清掉那一层的目标 */
  setGoal: (value: number | null, target: GoalTarget) => Promise<void>;
}

export function useWriting(options: WritingOptions): WritingState {
  const today = ref<WritingDay | null>(null);
  // 目标单独一个模块（偏好 vs 账本，变化理由不同）：这里只管"跟着会话转一下手"
  const daily = useDailyGoal({ read: options.transport.readGoal, write: options.transport.writeGoal }, options.workId);
  const goal = daily.goal;
  const defaultGoal = daily.defaultGoal;
  const visible = ref(false);
  const scope = ref<WritingScope>("work");
  const now = new Date();
  const month = ref({ year: now.getFullYear(), month: now.getMonth() + 1 });
  const days = ref<WritingDay[]>([]);
  const streak = ref(0);
  const busy = ref(false);
  const report = (error: unknown) => {
    options.onError?.(error instanceof Error ? error.message : String(error));
  };

  /** 问哪本书：看"全部"时给 null（核心那边 null 就是合计）。 */
  const queryWorkId = () => (scope.value === "all" ? null : options.workId.value);

  /** 日历要的区间：**显示的月份 ∪ 本周**——本周合计可能跨月，少取了就会算少。 */
  function rangeOf(): { from: string; to: string } {
    const shown = monthRange(month.value.year, month.value.month);
    const week = weekRange(new Date());
    return {
      from: shown.from < week.from ? shown.from : week.from,
      to: shown.to > week.to ? shown.to : week.to,
    };
  }

  async function loadCalendar(): Promise<void> {
    if (!options.workId.value && scope.value === "work") {
      days.value = [];
      streak.value = 0;
      return;
    }
    const { from, to } = rangeOf();
    const overview = await options.transport.overview(
      queryWorkId(),
      options.transport.tzOffsetMinutes(),
      from,
      to,
    );
    days.value = overview.days;
    streak.value = overview.streak;
  }

  return {
    today,
    goal,
    defaultGoal,
    visible,
    scope,
    month,
    days,
    streak,
    busy,
    refreshToday: async () => {
      try {
        today.value = await options.transport.today(
          options.workId.value,
          options.transport.tzOffsetMinutes(),
        );
      } catch (error) {
        report(error); // 今日进度刷不出来不该打断写作：只报一声，界面显示上一次的数
      }
    },
    loadGoal: async () => {
      try {
        await daily.load();
      } catch (error) {
        report(error);
      }
    },
    open: async () => {
      visible.value = true;
      // 每次打开都重读：跨零点、换书、在别处改过目标，打开时看到的都该是最新的
      month.value = { year: new Date().getFullYear(), month: new Date().getMonth() + 1 };
      busy.value = true;
      try {
        await daily.load();
        await loadCalendar();
      } catch (error) {
        report(error);
      } finally {
        busy.value = false;
      }
    },
    close: () => {
      visible.value = false;
    },
    setScope: async (next) => {
      if (scope.value === next) return;
      scope.value = next;
      busy.value = true;
      try {
        await loadCalendar();
      } catch (error) {
        report(error);
      } finally {
        busy.value = false;
      }
    },
    shiftMonth: async (delta) => {
      month.value = shiftMonth(month.value.year, month.value.month, delta);
      busy.value = true;
      try {
        await loadCalendar();
      } catch (error) {
        report(error);
      } finally {
        busy.value = false;
      }
    },
    setGoal: async (value, target) => {
      busy.value = true;
      try {
        await daily.set(value, target);
      } catch (error) {
        report(error);
      } finally {
        busy.value = false;
      }
    },
  };
}
