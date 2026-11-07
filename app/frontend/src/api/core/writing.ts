// 码字统计。

import { call, COMMANDS } from "./transport";

/** 某一天的字数（三个口径都给，界面按当前口径显示）。 */
export interface WritingDay {
  /** 本地日期 `2026-09-14` */
  day: string;
  chars: number;
  chars_no_punct: number;
  words: number;
}

/** 日历面板的全部数据：今日、区间内的每一天、连续天数。 */
export interface WritingOverview {
  today: WritingDay;
  /** 区间内**有记录**的日子（没记录的日子不返回，界面自己补齐） */
  days: WritingDay[];
  streak: number;
}

/** 今日码字（状态栏那行小字用）——`work_id` 给 null 就是全部作品合计。 */
export const writingToday = (work_id: number | null, tz_offset_minutes: number) =>
  call<WritingDay>(COMMANDS.writingToday, { work_id, tz_offset_minutes });

/** 日历数据：`from_day` / `to_day` 是本地日期（闭区间）。 */
export const writingOverview = (
  work_id: number | null,
  tz_offset_minutes: number,
  from_day: string,
  to_day: string,
) => call<WritingOverview>(COMMANDS.writingOverview, { work_id, tz_offset_minutes, from_day, to_day });
