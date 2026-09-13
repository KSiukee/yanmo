// 建书这一条线的接线验收：**先建书、再补简介与这本书的命名规则、最后开写**。
//
// "建书页上填的东西一次落好"是这一步的全部意义——作者填完不该再去别处补。
// 这里不碰界面、不碰核心（依赖注入替身），只盯次序与"不填就不写"。

import { test } from "node:test";
import assert from "node:assert/strict";
import { ref } from "vue";

import type { ExportAck, ShelfEntry } from "../api/core";
import { useShelf } from "./shelf.ts";

function harness() {
  const calls: string[] = [];
  const shelf = useShelf({
    transport: {
      list: async (): Promise<ShelfEntry[]> => [],
      create: async (kind: string, title: string) => {
        calls.push(`create:${kind}:${title}`);
        return 9;
      },
      rename: async () => {},
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
