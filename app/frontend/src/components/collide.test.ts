// 碰撞那一屏的纯函数验收：一对怎么认、关系怎么说、引子怎么拼、摘要怎么截。
//
// 与 `inspire.test.ts` 同一分工：这些函数的输出直接进界面，出错就是屏幕上看得见的毛病。
import { test } from "node:test";
import assert from "node:assert/strict";

import type { Collision, Spark } from "../api/inspire.ts";
import { t } from "../locales/index.ts";
import { collidePrompt, excerpt, pairKey, relationLabel } from "./collide.ts";

function spark(id: number, body: string): Spark {
  return {
    id,
    kind: "idea",
    body,
    source: "typed",
    anchors: [],
    importance: 0.5,
    created_at: 0,
    used_count: 0,
    gravity: {
      total: 1,
      importance: 0.5,
      heat: 1,
      relevance: 1,
      novelty: 1,
      anchor_hits: 0,
    },
  };
}

function pair(left: number, right: number, relation: "together" | "apart"): Collision {
  return {
    left: spark(left, `第${left}条`),
    right: spark(right, `第${right}条`),
    relation,
    shared_anchors: relation === "together" ? ["chapter:1"] : [],
    score: 1,
  };
}

test("一对的键把两条 id 都规范成小在前（两个方向认得是同一对）", () => {
  assert.equal(pairKey(pair(7, 3, "apart")), "3:7");
  assert.equal(pairKey(pair(3, 7, "apart")), "3:7");
  assert.equal(pairKey(pair(4, 4, "together")), "4:4");
});

test("两种关系都有界面说法（核心只给码）", () => {
  assert.equal(relationLabel("together"), t("collide.relation.together"));
  assert.equal(relationLabel("apart"), t("collide.relation.apart"));
  assert.notEqual(relationLabel("together"), "collide.relation.together");
  assert.notEqual(relationLabel("apart"), "collide.relation.apart");
});

test("引子按关系不同拼，两条原文原样嵌进去", () => {
  const together = collidePrompt(pair(1, 2, "together"));
  assert.ok(together.includes("第1条") && together.includes("第2条"));
  assert.equal(
    together,
    t("collide.prompt.together", { left: "第1条", right: "第2条" }),
  );
  const apart = collidePrompt(pair(1, 2, "apart"));
  assert.equal(apart, t("collide.prompt.apart", { left: "第1条", right: "第2条" }));
  assert.notEqual(together, apart, "两种关系的引子不该是同一句");
  assert.ok(!together.includes("{"), "占位符要全填上");
});

test("摘要把换行与连续空白压成一个空格", () => {
  assert.equal(excerpt("  雨夜\n\n那一场   要收线 "), "雨夜 那一场 要收线");
  assert.equal(excerpt(""), "");
});

test("摘要太长就截断（一屏要摆两条，扫得动才算数）", () => {
  const long = "甲".repeat(120);
  const short = excerpt(long);
  assert.equal(short.length, 61, "60 个字 + 一个省略号");
  assert.ok(short.endsWith("…"));
});
