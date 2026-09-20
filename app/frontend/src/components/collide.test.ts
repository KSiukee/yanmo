// 碰撞那一屏的纯函数验收：一对怎么认、两头怎么说、引子怎么拼、摘要怎么截。
//
// 与 `inspire.test.ts` 同一分工：这些函数的输出直接进界面，出错就是屏幕上看得见的毛病。
import { test } from "node:test";
import assert from "node:assert/strict";

import type { Collision, CollisionKind, CollisionSideView, Spark } from "../api/inspire.ts";
import { t } from "../locales/index.ts";
import { collidePrompt, excerpt, pairKey, relationLabel, sideLabel, sideWhenText } from "./collide.ts";

function spark(id: number, body: string, created_at = 0): Spark {
  return {
    id,
    kind: "idea",
    body,
    source: "typed",
    anchors: [],
    importance: 0.5,
    created_at,
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

function sparkSide(id: number, body: string): CollisionSideView {
  return { key: `spark:${id}`, kind: "spark", text: body, note: "", spark: spark(id, body, 1700000000000) };
}

function elementSide(kind: CollisionKind, id: number, text: string, note = ""): CollisionSideView {
  return { key: `${kind}:${id}`, kind, text, note, spark: null };
}

function pair(left: CollisionSideView, right: CollisionSideView, relation: "together" | "apart"): Collision {
  return { left, right, relation, shared_anchors: relation === "together" ? ["chapter:1"] : [], score: 1 };
}

test("一对的键把两头的规范标识接起来（顺序由核心定，界面不自己排）", () => {
  const one = pair(sparkSide(7, "甲"), elementSide("plan", 3, "乙"), "together");
  assert.equal(pairKey(one), "spark:7|plan:3");
  const other = pair(sparkSide(5, "丙"), elementSide("plan", 4, "丁"), "apart");
  assert.notEqual(pairKey(one), pairKey(other));
});

test("四种撞头都有界面说法（核心只给码）", () => {
  for (const kind of ["spark", "plan", "foreshadow", "entity"] as const) {
    assert.equal(sideLabel(kind), t(`collide.side.${kind}`));
    assert.notEqual(sideLabel(kind), `collide.side.${kind}`, `${kind} 的文案没找着`);
  }
});

test("两种关系都有界面说法（核心只给码）", () => {
  assert.equal(relationLabel("together"), t("collide.relation.together"));
  assert.equal(relationLabel("apart"), t("collide.relation.apart"));
  assert.notEqual(relationLabel("together"), "collide.relation.together");
  assert.notEqual(relationLabel("apart"), "collide.relation.apart");
});

test("引子按关系不同拼，两头那段字原样嵌进去（碎片与元素一视同仁）", () => {
  const mixed = pair(sparkSide(1, "雨夜那一场"), elementSide("plan", 9, "这一章要摊牌"), "together");
  assert.equal(
    collidePrompt(mixed),
    t("collide.prompt.together", { left: "雨夜那一场", right: "这一章要摊牌" }),
  );
  const apart = pair(sparkSide(1, "雨夜那一场"), elementSide("entity", 9, "李三"), "apart");
  assert.equal(apart.left.text, "雨夜那一场");
  assert.equal(collidePrompt(apart), t("collide.prompt.apart", { left: "雨夜那一场", right: "李三" }));
  assert.notEqual(collidePrompt(mixed), collidePrompt(apart), "两种关系的引子不该是同一句");
  assert.ok(!collidePrompt(mixed).includes("{"), "占位符要全填上");
});

test("只有碎片那一头有'多会儿记下的'（元素是本来就有的）", () => {
  assert.notEqual(sideWhenText(sparkSide(1, "甲")), "");
  assert.equal(sideWhenText(elementSide("plan", 1, "乙")), "");
  assert.equal(sideWhenText(elementSide("foreshadow", 1, "丙", "备注")), "");
});

test("摘要把换行与连续空白压成一个空格", () => {
  assert.equal(excerpt("  雨夜\n\n那一场   要收线 "), "雨夜 那一场 要收线");
  assert.equal(excerpt(""), "");
});

test("摘要太长就截断（一屏要摆两头，扫得动才算数）", () => {
  const long = "甲".repeat(120);
  const short = excerpt(long);
  assert.equal(short.length, 61, "60 个字 + 一个省略号");
  assert.ok(short.endsWith("…"));
});
