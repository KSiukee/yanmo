// 展示口径的测试：**数字与时间怎么念**，只此一处说了算。

import { test } from "node:test";
import assert from "node:assert/strict";

import { formatBytes, formatWhen, formatWords, localDay, localDaysBetween } from "./display.ts";
import { nextWorkAfterDelete, shelfKindLabel, shelfLabel } from "./shelf.ts";
import type { ShelfEntry } from "../api/core.ts";

const DAY = 86_400_000;

function entry(id: number, chapters: number, words: number, opened: number | null = null): ShelfEntry {
  return {
    id,
    kind: "novel",
    title: `第${id}本`,
    chapters,
    word_count: words,
    char_count: words,
    chars_no_punct: words,
    opened_at: opened,
    created_at: 0,
    updated_at: 0,
  };
}

test("字数给人看：一万以上换成「万」", () => {
  assert.equal(formatWords(0), "0");
  assert.equal(formatWords(9999), "9999");
  assert.equal(formatWords(10000), "1万");
  assert.equal(formatWords(12345), "1.2万");
  assert.equal(formatWords(1234567), "123.5万");
});

test("最近打开时间给人看：今天 / 昨天 / N 天前 / 日期 / 从没打开", () => {
  const now = 1_700_000_000_000;
  assert.equal(formatWhen(null, now), "还没打开过");
  assert.equal(formatWhen(now - 1000, now), "今天");
  assert.equal(formatWhen(now - DAY, now), "昨天");
  assert.equal(formatWhen(now - 5 * DAY, now), "5 天前");
  assert.match(formatWhen(now - 100 * DAY, now), /^\d{4}-\d{2}-\d{2}$/, "太久远就报日期");
});

// 2026-09-15 代码质量评审：轻微 27——"今天/昨天"必须按**本地日历日**算，不是毫秒差。
// 凌晨 0:30 看昨晚 23:50 打开的书，毫秒差只有 40 分钟，但那是"昨天"。
test("跨零点：按本地日历日算「昨天」，不拿毫秒差除 24 小时", () => {
  const lateNight = new Date(2026, 8, 14, 23, 50, 0); // 9 月 14 日 23:50
  const afterMidnight = new Date(2026, 8, 15, 0, 30, 0); // 9 月 15 日 00:30
  assert.equal(
    formatWhen(lateNight.getTime(), afterMidnight.getTime()),
    "昨天",
    "只差 40 分钟也算昨天（过了本地零点）"
  );
  // 同一天早上的记录，晚上看还是"今天"
  assert.equal(
    formatWhen(new Date(2026, 8, 15, 8, 0, 0).getTime(), new Date(2026, 8, 15, 23, 0, 0).getTime()),
    "今天"
  );
  // 前天同样按日历日数
  assert.equal(
    formatWhen(new Date(2026, 8, 13, 23, 50, 0).getTime(), new Date(2026, 8, 15, 0, 10, 0).getTime()),
    "2 天前"
  );
  assert.equal(localDaysBetween(afterMidnight.getTime(), afterMidnight.getTime()), 0);
});

// 库里的时间戳坏了（NaN / 超出 Date 范围）是可能的：**不许把 "NaN-NaN-NaN" 摆到界面上**，
// 也不许把一个很远的未来时间说成"今天"。
test("时间戳坏了就说「时间未知」，绝不显示 NaN", () => {
  const now = 1_700_000_000_000;
  assert.equal(formatWhen(NaN, now), "时间未知");
  assert.equal(formatWhen(Infinity, now), "时间未知");
  assert.equal(formatWhen(-Infinity, now), "时间未知");
  assert.equal(formatWhen(9e18, now), "时间未知", "超出 Date 能表示的范围");
});

test("字数不是有限数就说「字数未知」，不显示 NaN", () => {
  assert.equal(formatWords(NaN), "字数未知");
  assert.equal(formatWords(Infinity), "字数未知");
});

test("书架上那一行的小字：有章报章数，单篇只报字数，且**跟着当前口径**", () => {
  const counts = { chapters: 12, word_count: 34000, char_count: 41000, chars_no_punct: 36000 };
  assert.equal(shelfLabel(counts, "chars"), "12 章 · 4.1万 字");
  assert.equal(shelfLabel({ ...counts, chapters: 0 }, "chars_no_punct"), "3.6万 字");
  // 按词口径下单位是「词」——模板里写死「字」就会串味
  assert.equal(shelfLabel(counts, "words"), "12 章 · 3.4万 词");
});

test("作品类型的显示名", () => {
  assert.equal(shelfKindLabel("novel"), "长篇");
  assert.equal(shelfKindLabel("collection"), "短篇集");
  assert.equal(shelfKindLabel("article"), "单篇");
  assert.equal(shelfKindLabel("whatever"), "单篇", "不认识的取值当单篇，不崩");
});

test("删掉当前这本之后：开列表里的下一本；一本不剩交给核心", () => {
  const shelf = [entry(3, 1, 10), entry(2, 1, 10), entry(1, 1, 10)];
  assert.equal(nextWorkAfterDelete(shelf, 3), 2, "删的是第一本，就开它后面那本");
  assert.equal(nextWorkAfterDelete(shelf, 2), 3, "删的是中间那本，就开排最前的那本");
  assert.equal(nextWorkAfterDelete([entry(9, 1, 10)], 9), null, "一本不剩 → 交给核心建默认的");
});

test("容量给人看：KB / MB / GB 换算对，不出现「几十 GB 显示成 11 MB」那种错", () => {
  assert.equal(formatBytes(0), "", "没有数就不显示（不写 0 KB 糊人）");
  assert.equal(formatBytes(512 * 1024), "512 KB");
  assert.equal(formatBytes(1024 ** 2 * 3), "3 MB");
  assert.equal(formatBytes(1024 ** 3 * 11.5), "11.5 GB");
  assert.equal(formatBytes(1024 ** 3 * 102), "102 GB");
  // 量纲错一位是这类显示最常见的 bug：50 GB 绝不该显示成 MB 级的小数
  assert.match(formatBytes(50 * 1024 ** 3), /GB$/);
});

test("本地那一天：按作者本地的年月日算，不是 UTC", () => {
  // 本地时间 2026-09-16 早上八点：按月日算就是 20260916
  assert.equal(localDay(new Date(2026, 8, 16, 8, 0, 0)), 20260916);
  // 跨零点前后是两天（按天重置的账靠的就是它）
  assert.equal(localDay(new Date(2026, 8, 16, 23, 59, 59)), 20260916);
  assert.equal(localDay(new Date(2026, 8, 17, 0, 0, 1)), 20260917);
  // 月与日都要补零到两位，拼出来才是同一个格式
  assert.equal(localDay(new Date(2026, 0, 5, 12, 0, 0)), 20260105);
});
