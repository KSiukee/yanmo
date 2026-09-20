// 「灵感」那一片的纯函数验收：落点怎么说、引力凭什么、场景卡叫什么。
//
// 与 `creator.test.ts` / `question.test.ts` 同一分工：这些函数的输出直接进界面，
// 而且"落点与稳定码一一对上"这件事**只有这里盯着**（错了界面上就是一片键名）。
import { test } from "node:test";
import assert from "node:assert/strict";

import { t } from "../locales/index.ts";
import { gravityLabel, sceneTitleFrom, targetLabel } from "./inspire.ts";

test("四个落点都有界面说法，且与核心的稳定码一一对上", () => {
  assert.equal(targetLabel("body"), t("inspire.target.body"));
  assert.equal(targetLabel("outline"), t("inspire.target.outline"));
  assert.equal(targetLabel("scene"), t("inspire.target.scene"));
  assert.equal(targetLabel("event"), t("inspire.target.event"));
  // 查不到就原样返回键名——那说明稳定码与字典脱节了，这里当场钉住
  for (const target of ["body", "outline", "scene", "event"] as const) {
    assert.notEqual(targetLabel(target), `inspire.target.${target}`);
  }
});

test("引力拆解把四项换成整数百分比摆出来", () => {
  const line = gravityLabel({
    total: 0.3,
    importance: 0.5,
    heat: 0.606,
    relevance: 1,
    novelty: 1,
    anchor_hits: 1,
  });
  assert.equal(
    line,
    t("inspire.gravity", { importance: 50, heat: 61, relevance: 100, novelty: 100 }),
  );
  assert.ok(!line.includes("{"), "占位符要全填上，界面上不许露出来");
});

test("场景卡的名字取第一行；空正文交回空串（那一路留给命名规则）", () => {
  assert.equal(sceneTitleFrom("\n  城堡的雨夜  \n后半段"), "城堡的雨夜");
  assert.equal(sceneTitleFrom("   "), "");
  assert.equal(sceneTitleFrom(""), "");
});

test("太长的第一行截断到上限（不然树上会挂一整段）", () => {
  const long = "甲".repeat(60);
  const title = sceneTitleFrom(long);
  assert.equal(title.length, 41, "40 个字 + 一个省略号");
  assert.ok(title.endsWith("…"));
});
