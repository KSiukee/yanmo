// 建书这一条线的接线验收：**先建书、再补简介与这本书的命名规则、最后开写**。
//
// "建书页上填的东西一次落好"是这一步的全部意义——作者填完不该再去别处补。
// 这里不碰界面、不碰核心（依赖注入替身），只盯次序与"不填就不写"。

import { test } from "node:test";
import assert from "node:assert/strict";
import { ref } from "vue";

import type { ExportAck, ShelfEntry } from "../api/core";
import { looksLikeFirstRun, useShelf } from "./shelf.ts";

/** 核心首启自动建的那本**无名空壳**（只有一本、没名字、一个字没写）。 */
function shellEntry(over: Partial<ShelfEntry> = {}): ShelfEntry {
  return {
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
  } as ShelfEntry;
}

function harness(onError?: (message: string) => void, listed: ShelfEntry[] = []) {
  const calls: string[] = [];
  const shelf = useShelf({
    transport: {
      list: async (): Promise<ShelfEntry[]> => listed,
      create: async (kind: string, title: string) => {
        calls.push(`create:${kind}:${title}`);
        return 9;
      },
      rename: async (work_id: number, title: string) => {
        calls.push(`rename:${work_id}:${title}`);
      },
      remove: async (work_id: number) => {
        calls.push(`remove:${work_id}`);
      },
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
  const entry = shellEntry;

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

test("建了自己的书：第一次打开自动建的那本空壳顺手收进回收站，并且**说一声**", async () => {
  const shell = shellEntry();
  const { shelf, calls } = harness(undefined, [shell]);
  await shelf.create({ kind: "novel", title: "长夜", summary: "", naming: null });
  assert.deepEqual(
    calls,
    ["create:novel:长夜", "open:9", "remove:1"],
    "建完自己的书、切过去之后，才把那本空壳收走",
  );
  assert.match(shelf.note.value, /回收站/, "收走了要说一声（可捞回），别悄悄动书架");
});

test("已经起过名的书：建新书时不许动它", async () => {
  const named = { id: 1, title: "旧书", word_count: 0 } as ShelfEntry;
  const { shelf, calls } = harness(undefined, [named]);
  await shelf.create({ kind: "novel", title: "长夜", summary: "", naming: null });
  assert.deepEqual(calls, ["create:novel:长夜", "open:9"], "有名字的书不是「空壳」，一个字都不许动");
});

test("写过一个字的书：建新书时也不许动它", async () => {
  const written = { id: 1, title: "", word_count: 12 } as ShelfEntry;
  const { shelf, calls } = harness(undefined, [written]);
  await shelf.create({ kind: "novel", title: "长夜", summary: "", naming: null });
  assert.deepEqual(calls, ["create:novel:长夜", "open:9"], "写过字的更不许动");
});

// ── 首启提示的第二个入口：接手那本空壳（**不另建一本**）────────────────────────

test("接手空壳（类型没改）：名字落在**原来那本**上，一本都不新建", async () => {
  const shell = shellEntry();
  const { shelf, calls } = harness(undefined, [shell]);
  await shelf.refresh(); // 提示条上的入口用的是**已经拉回来的书架**（启动时就会拉一次）
  shelf.openAdoptShell();
  assert.deepEqual(shelf.form.value, { mode: "adopt", entry: shell });
  await shelf.adopt(shell.id, { kind: "article", title: "  长夜  ", summary: "  一句话  ", naming: null });
  assert.deepEqual(
    calls,
    ["rename:1:长夜", "summary:1:一句话"],
    "同类型就该就地改名：没有 create，也没有 remove（作者要的正是「别另建一本」）",
  );
});

test("接手空壳（换成长篇）：按新类型另建一本，空壳照旧收进回收站并说一声", async () => {
  const { shelf, calls } = harness(undefined, [shellEntry()]);
  await shelf.adopt(1, { kind: "novel", title: "长夜", summary: "", naming: "chinese" });
  assert.deepEqual(calls, ["create:novel:长夜", "naming:9:chinese", "open:9", "remove:1"]);
  assert.match(shelf.note.value, /回收站/, "收走了要说一声（可捞回）");
});

test("接手空壳：空名字当场拦住，一个字都不写库", async () => {
  let reported = "";
  const { shelf, calls } = harness((message) => {
    reported = message;
  }, [shellEntry()]);
  await shelf.adopt(1, { kind: "article", title: "   ", summary: "随便", naming: null });
  assert.deepEqual(calls, [], "空书名不该落到核心");
  assert.equal(reported, "书名不能为空");
});

test("已经不是首启那本（起过名 / 写过字 / 建了第二本）：没有「接手」这条入口", async () => {
  const named = harness(undefined, [shellEntry({ title: "旧书" })]);
  await named.shelf.refresh();
  named.shelf.openAdoptShell();
  assert.equal(named.shelf.form.value, null, "有名字的书不是空壳");

  const written = harness(undefined, [shellEntry({ word_count: 5 })]);
  await written.shelf.refresh();
  written.shelf.openAdoptShell();
  assert.equal(written.shelf.form.value, null, "写过字的更不是");

  const two = harness(undefined, [shellEntry(), shellEntry({ id: 2 })]);
  await two.shelf.refresh();
  two.shelf.openAdoptShell();
  assert.equal(two.shelf.form.value, null, "已经有两本了，谈不上「第一次使用」");
});
