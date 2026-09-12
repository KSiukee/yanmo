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
    t("gap.where_in_volume"); // 这条文案要 {parent}，故意不给
  } finally {
    console.warn = original;
  }
  assert.equal(warnings.length, 1);
  assert.ok(String(warnings[0].join(" ")).includes("parent"), "报警里要写清缺的是哪个参数");
});

test("参数给全了就不该报警", () => {
  const warnings: unknown[][] = [];
  const original = console.warn;
  console.warn = (...args: unknown[]) => warnings.push(args);
  try {
    assert.equal(t("gap.where_in_volume", { parent: "第一卷" }), "「第一卷」里缺了");
    // 同一句再渲染一次也不该重复报（只报一次，免得刷屏）
    t("gap.where_in_volume", { parent: "第二卷" });
  } finally {
    console.warn = original;
  }
  assert.equal(warnings.length, 0);
});
