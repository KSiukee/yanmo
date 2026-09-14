// 建书这一条线的接线验收：**先建书、再补简介与这本书的命名规则、最后开写**。
//
// "建书页上填的东西一次落好"是这一步的全部意义——作者填完不该再去别处补。
// 这里不碰界面、不碰核心（依赖注入替身），只盯次序与"不填就不写"。

import { test } from "node:test";
import assert from "node:assert/strict";
import { ref } from "vue";

import type { ExportAck, ShelfEntry } from "../api/core";
import { looksLikeFirstRun, useShelf } from "./shelf.ts";

function harness(onError?: (message: string) => void) {
  const calls: string[] = [];
  const shelf = useShelf({
    transport: {
      list: async (): Promise<ShelfEntry[]> => [],
      create: async (kind: string, title: string) => {
        calls.push(`create:${kind}:${title}`);
        return 9;
      },
      rename: async (work_id: number, title: string) => {
        calls.push(`rename:${work_id}:${title}`);
      },
      remove: async () => {},
      export: async (): Promise<ExportAck> => ({ path: "x" }),
      writeSummary: async (work_id: number, summary: string) => {
        calls.push(`summary:${work_id}:${summary}`);
      },
      writeNaming: async (work_id: number, naming: string) => {
        calls.push(`naming:${work_id}:${naming}`);
      },
    },
    workId: ref<number | null>(null),
    openWork: async (work_id: number | null) => {
      calls.push(`open:${work_id}`);
      return true;
    },
    onError,
  });
  return { shelf, calls };
}

test("建书页填齐了：建 → 简介 → 命名规则 → 开写", async () => {
  const { shelf, calls } = harness();
  await shelf.create({ kind: "novel", title: "长夜", summary: "一个人的夜路。", naming: "chinese" });
  assert.deepEqual(calls, [
    "create:novel:长夜",
    "summary:9:一个人的夜路。",
    "naming:9:chinese",
    "open:9",
  ]);
});

test("没填简介 / 没选命名规则：不写多余的覆盖", async () => {
  const { shelf, calls } = harness();
  await shelf.create({ kind: "collection", title: "故园随笔", summary: "   ", naming: null });
  assert.deepEqual(calls, ["create:collection:故园随笔", "open:9"], "空简介与跟随设置都不写库");
});

test("简介两头的空白不算内容", async () => {
  const { shelf, calls } = harness();
  await shelf.create({ kind: "article", title: "小记", summary: "  短短一句。  ", naming: null });
  assert.deepEqual(calls, ["create:article:小记", "summary:9:短短一句。", "open:9"]);
});

// ── 作品表单统一与首启引导的判据 ────────────────────────────────────────────

test("首启判据：只有一本、没名字、一个字没写——才给那条引导", () => {
  const entry = (over: Partial<ShelfEntry>): ShelfEntry =>
    ({
      id: 1,
      kind: "article",
      title: "",
      summary: "",
      word_count: 0,
      char_count: 0,
      chars_no_punct: 0,
      chapters: 0,
      opened_at: 0,
      created_at: 0,
      updated_at: 0,
      ...over,
    }) as ShelfEntry;

  // 核心首启建的那本无名空壳：是首启态
  assert.equal(looksLikeFirstRun([entry({})]), true);
  // 起过名字了：不是（作者已经开始用了）
  assert.equal(looksLikeFirstRun([entry({ title: "长夜" })]), false);
  // 写过一个字：不是
  assert.equal(looksLikeFirstRun([entry({ word_count: 12 })]), false);
  // 建了第二本：不是
  assert.equal(looksLikeFirstRun([entry({}), entry({ id: 2 })]), false);
  // 一本都没有（理论上到不了，但判据要稳）：不是
  assert.equal(looksLikeFirstRun([]), false);
});

test("编辑作品：改名 + 简介一次落好（空书名当场拦住，不惊动核心）", async () => {
  const { shelf, calls } = harness();
  await shelf.edit(7, { title: "  新名字  ", summary: "  一句话  " });
  assert.deepEqual(calls, ["rename:7:新名字", "summary:7:一句话"]);

  // 空书名：**不进核心**（核心会回 word.title_empty，那是"写错地方"的错，
  // 不是作者做错了什么——在表单这一层给一句他看得懂的话）
  let reported = "";
  const guarded = harness((message) => {
    reported = message;
  });
  await guarded.shelf.edit(7, { title: "   ", summary: "随便" });
  assert.deepEqual(guarded.calls, [], "空书名不该落到核心");
  assert.equal(reported, "书名不能为空");
});

test("作品表单的开合：书架与首启提示都能打开它（新建 / 编辑两种模式）", () => {
  const { shelf } = harness();
  assert.equal(shelf.form.value, null);
  shelf.openCreate();
  assert.deepEqual(shelf.form.value, { mode: "create" });
  shelf.closeForm();
  assert.equal(shelf.form.value, null);
  const entry = { id: 3, title: "长夜" } as ShelfEntry;
  shelf.openEdit(entry);
  assert.deepEqual(shelf.form.value, { mode: "edit", entry });
  shelf.closeForm();
  assert.equal(shelf.form.value, null);
});
