// 「文案与代码脱节」的当场报警：单独一个文件跑，别被别处的字典体检先触发掉
// （那个限制是"同一个模板只报一次"，同进程里先跑过的测试会把它消掉）。

import { test } from "node:test";
import assert from "node:assert/strict";

import { t } from "./index.ts";

test("缺参数要当场喊一声，而不是把 {占位符} 静默留在界面上", () => {
  const warnings: unknown[][] = [];
  const original = console.warn;
  console.warn = (...args: unknown[]) => warnings.push(args);
  try {
    t("display.days_ago"); // 这条文案要 {days}，故意不给
  } finally {
    console.warn = original;
  }
  assert.equal(warnings.length, 1);
  assert.ok(String(warnings[0].join(" ")).includes("days"), "报警里要写清缺的是哪个参数");
});

test("参数给全了就不该报警", () => {
  const warnings: unknown[][] = [];
  const original = console.warn;
  console.warn = (...args: unknown[]) => warnings.push(args);
  try {
    assert.equal(t("display.days_ago", { days: 3 }), "3 天前");
    // 同一句再渲染一次也不该重复报（只报一次，免得刷屏）
    t("display.days_ago", { days: 9 });
  } finally {
    console.warn = original;
  }
  assert.equal(warnings.length, 0);
});
