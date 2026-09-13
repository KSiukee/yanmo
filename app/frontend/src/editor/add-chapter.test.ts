// 点「+」之后的编排：**建在同层、落在点的那一行后面**，并保证"连点"是接着往下排。
//
// 这里测的是编排本身（依赖全部注入，不碰界面、不碰核心）：
// - 章上点「+」→ 锚在那一章后面插一章（**切章由会话那条路负责**：它建完就切过去）；
// - 容器上点「+」→ 往里加一个（标题交给核心的模板），并切过去；
// - **同一行连点** → 每次锚在"上一次建出来的那一章"后面，所以是一路往下排，不会倒着长；
// - 换一行点 → 锚点回到新点的这一行（位置仍旧"点哪儿插哪儿"）。

import { test } from "node:test";
import assert from "node:assert/strict";

import { useAddChapter } from "./add-chapter.ts";

/** 一棵假树：章（chapter）挂在卷（volume）下面。 */
function row(id: number, kind: string, parent_id: number | null) {
  return {
    id,
    parent_id,
    kind,
    title: `第{$N}章`,
    title_rendered: "第1章",
    word_count: 0,
    char_count: 0,
    chars_no_punct: 0,
    has_body: false,
    holds_body: kind !== "volume",
    accepts_children: kind === "volume",
    has_children: false,
    chapter_count: 0,
    subtree_word_count: 0,
    subtree_char_count: 0,
    subtree_chars_no_punct: 0,
    depth: 1,
    expanded: false,
  };
}

function harness() {
  const created: Array<{ from: number | null; anchor: number | null }> = [];
  const opened: number[] = [];
  let next_id = 100;
  const adding = useAddChapter({
    directory: {
      create: async (parent_id, kind, title) => {
        created.push({ from: parent_id, anchor: null });
        void kind;
        void title;
        return next_id++;
      },
      refresh: async () => {},
    },
    openFreshChapter: async (node_id) => {
      opened.push(node_id);
    },
    addChapterAfter: async (node_id) => {
      created.push({ from: null, anchor: node_id });
      return next_id++;
    },
  });
  return { adding, created, opened };
}

test("章上点「+」：锚在那一章后面插一章（切章交给会话那条路）", async () => {
  const { adding, created, opened } = harness();
  await adding.addHere(row(7, "chapter", 1));
  assert.deepEqual(created, [{ from: null, anchor: 7 }], "锚在点的那一章后面");
  assert.deepEqual(opened, [], "切章由会话的 addChapterAfter 负责（它建完就切），这里不重复切");
});

test("容器上点「+」：往里加一章（标题交给核心的模板）并打开", async () => {
  const { adding, created, opened } = harness();
  await adding.addHere(row(3, "volume", null));
  assert.deepEqual(created, [{ from: 3, anchor: null }], "往容器里面加");
  assert.equal(opened.length, 1);
});

test("同一行连点：新章接着上一次那一章往下排（不是插在同一处倒着长）", async () => {
  const { adding, created } = harness();
  const chapter = row(7, "chapter", 1);
  await adding.addHere(chapter);
  await adding.addHere(chapter);
  await adding.addHere(chapter);
  assert.deepEqual(
    created.map((item) => item.anchor),
    [7, 100, 101],
    "第二次锚在第一次建出来的那章（100）后面，第三次锚在 101 后面——一路往下",
  );
});

test("换一行点「+」：锚点回到新点的这一行（点哪儿插哪儿）", async () => {
  const { adding, created } = harness();
  await adding.addHere(row(7, "chapter", 1));
  await adding.addHere(row(9, "chapter", 1));
  assert.deepEqual(created.map((item) => item.anchor), [7, 9]);
});

test("建不出来（返回 null）：不切章，也不算续接点", async () => {
  const opened: number[] = [];
  const anchors: Array<number | null> = [];
  const adding = useAddChapter({
    directory: { create: async () => null, refresh: async () => {} },
    openFreshChapter: async (node_id) => {
      opened.push(node_id);
    },
    addChapterAfter: async (node_id) => {
      anchors.push(node_id);
      return null;
    },
  });
  const chapter = row(7, "chapter", 1);
  await adding.addHere(chapter);
  await adding.addHere(chapter);
  assert.deepEqual(opened, [], "没建成就不切章");
  assert.deepEqual(anchors, [7, 7], "没建成就不算续接点，下一次仍旧锚在原来那一行");
});
