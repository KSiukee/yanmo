// 目录树的测试：**懒加载只拉该拉的、结构编辑后看得见、拖拽不会把树弄坏**。
//
// 替身里放一份"参考实现"的扁平清单（谁是第几个孩子），树的每次动作都与它对账——
// 这样测的不是"代码写了什么"，而是"树显示出来的顺序对不对"。

import { test } from "node:test";
import assert from "node:assert/strict";

import type { TreeNode } from "../api/core.ts";
import { addIntent, containerLabel, DirectoryTree, type TreeTransport } from "./tree.ts";

interface Spec {
  id: number;
  parent: number | null;
  kind: string;
  title: string;
  words?: number;
}

/** 替身也要照着核心的规矩来：谁能写正文、谁能收下级（与 yanmo-core 的取值一致）。 */
const HOLDS_BODY = new Set(["chapter", "section", "piece", "scene"]);
const ACCEPTS_CHILDREN = new Set(["volume", "chapter", "section"]);

function fakeWorld(initial: Spec[]) {
  const calls: string[] = [];
  let specs = initial.map((spec) => ({ ...spec }));
  let next_id = Math.max(0, ...initial.map((spec) => spec.id)) + 1;

  const toDto = (spec: Spec): TreeNode => {
    const under = subtreeTotals(spec.id);
    return {
      id: spec.id,
      parent_id: spec.parent,
      kind: spec.kind,
      title: spec.title,
      title_rendered: renderTitle(spec),
      word_count: spec.words ?? 0,
      char_count: spec.words ?? 0,
      chars_no_punct: spec.words ?? 0,
      has_body: (spec.words ?? 0) > 0,
      holds_body: HOLDS_BODY.has(spec.kind),
      accepts_children: ACCEPTS_CHILDREN.has(spec.kind),
      has_children: specs.some((kid) => kid.parent === spec.id),
      chapter_count: under.chapters,
      subtree_word_count: under.words,
      subtree_char_count: under.words,
      subtree_chars_no_punct: under.words,
    };
  };

  /** 替身的"连子树一起删"：跟核心一样，删一个节点会带走它下面所有节点。 */
  const subtreeIds = (root: number): number[] => {
    const out = [root];
    for (const kid of specs.filter((item) => item.parent === root)) out.push(...subtreeIds(kid.id));
    return out;
  };

  /** 参考实现：替身自己也算一遍"本卷几章 / 共多少字"，跟树的数字对账。 */
  const subtreeTotals = (root: number): { chapters: number; words: number } => {
    let chapters = 0;
    let words = 0;
    for (const kid of specs.filter((item) => item.parent === root)) {
      const inner = subtreeTotals(kid.id);
      chapters += inner.chapters + (kid.kind === "chapter" ? 1 : 0);
      words += (kid.words ?? 0) + inner.words;
    }
    return { chapters, words };
  };

  /**
   * 替身版的"渲染标题"：把 `{$N}` 换成本层同类里的序号（核心那份更讲究，这里只求"会变"，
   * 好让测试能分辨"拉回来的是核心渲染的那份"还是"界面自己就地拼的"）。
   */
  const renderTitle = (spec: Spec): string => {
    const siblings = specs.filter((item) => item.parent === spec.parent && item.kind === spec.kind);
    const ordinal = siblings.findIndex((item) => item.id === spec.id) + 1;
    return spec.title.replace("{$N}", String(ordinal));
  };

  const transport: TreeTransport = {
    children: async (work_id, parent_id) => {
      calls.push(`children:${work_id}:${parent_id ?? "-"}`);
      return specs.filter((spec) => spec.parent === parent_id).map(toDto);
    },
    create: async (_work_id, parent_id, kind, title) => {
      const id = next_id++;
      specs.push({ id, parent: parent_id, kind, title });
      calls.push(`create:${parent_id ?? "-"}:${title}`);
      return id;
    },
    ancestors: async (node_id) => {
      calls.push(`ancestors:${node_id}`);
      const chain: number[] = [];
      let current = specs.find((item) => item.id === node_id)?.parent ?? null;
      while (current !== null) {
        chain.push(current);
        current = specs.find((item) => item.id === current)?.parent ?? null;
      }
      return chain.reverse(); // 根在前
    },
    remove: async (node_id) => {
      calls.push(`remove:${node_id}`);
      const doomed = subtreeIds(node_id);
      specs = specs.filter((item) => !doomed.includes(item.id));
      return doomed.length;
    },
    rename: async (node_id, title) => {
      calls.push(`rename:${node_id}:${title}`);
      const spec = specs.find((item) => item.id === node_id);
      if (spec) spec.title = title;
    },
    move: async (node_id, parent_id, index) => {
      calls.push(`move:${node_id}:${parent_id ?? "-"}:${index}`);
      const moved = specs.find((item) => item.id === node_id);
      if (!moved) return;
      specs = specs.filter((item) => item.id !== node_id);
      const siblings = specs.filter((item) => item.parent === parent_id);
      const at = Math.max(0, Math.min(index, siblings.length));
      moved.parent = parent_id;
      if (at >= siblings.length) specs.push(moved);
      else specs.splice(specs.indexOf(siblings[at]), 0, moved);
    },
  };

  return { transport, calls };
}

/** 一卷两章 + 第二章下挂一张场景卡。 */
const BOOK: Spec[] = [
  { id: 1, parent: null, kind: "volume", title: "第一卷" },
  { id: 2, parent: 1, kind: "chapter", title: "第一章", words: 1200 },
  { id: 3, parent: 1, kind: "chapter", title: "第二章", words: 800 },
  { id: 4, parent: 3, kind: "scene", title: "场景卡" },
  { id: 5, parent: null, kind: "volume", title: "第二卷" },
];

test("打开作品只拉根那一层：卷里的章不跟着进来", async () => {
  const { transport, calls } = fakeWorld(BOOK);
  const tree = new DirectoryTree(transport);

  await tree.openWork(7);
  assert.deepEqual(calls, ["children:7:-"]);
  assert.deepEqual(
    tree.rows().map((row) => row.title),
    ["第一卷", "第二卷"],
    "没展开的卷，里面的章不该出现",
  );
  assert.equal(tree.rows()[0].has_children, true, "有下级的行要能画出展开箭头");
  assert.equal(tree.rows()[0].depth, 0);
});

test("展开一层才拉一层；再展开孙辈又是一次", async () => {
  const { transport, calls } = fakeWorld(BOOK);
  const tree = new DirectoryTree(transport);
  await tree.openWork(7);

  await tree.toggle(1);
  assert.deepEqual(calls.slice(1), ["children:7:1"], "展开第一卷只该拉第一卷");
  assert.deepEqual(
    tree.rows().map((row) => `${row.depth}:${row.title}`),
    ["0:第一卷", "1:第一章", "1:第二章", "0:第二卷"],
  );

  await tree.toggle(3);
  assert.deepEqual(calls.slice(2), ["children:7:3"]);
  assert.deepEqual(
    tree.rows().map((row) => row.title),
    ["第一卷", "第一章", "第二章", "场景卡", "第二卷"],
    "场景卡挂在第二章下面",
  );
});

test("收起就不显示子层；再展开会重拉一次（期间可能被别处改过）", async () => {
  const { transport, calls } = fakeWorld(BOOK);
  const tree = new DirectoryTree(transport);
  await tree.openWork(7);
  await tree.toggle(1);
  await tree.toggle(1);

  assert.deepEqual(
    tree.rows().map((row) => row.title),
    ["第一卷", "第二卷"],
  );
  assert.equal(tree.rows()[0].expanded, false);
  const before = calls.length;
  await tree.toggle(1);
  assert.equal(calls.length, before + 1, "重开这一层会重拉一次（期间可能被别处改过）");
});

test("新建：重拉父层，返回新 id，并顺手展开父层", async () => {
  const { transport, calls } = fakeWorld(BOOK);
  const tree = new DirectoryTree(transport);
  await tree.openWork(7);

  const created = await tree.create(1, "chapter", "第三章");
  assert.equal(created, 6);
  assert.deepEqual(calls.slice(1), ["create:1:第三章", "children:7:-", "children:7:1"]);
  assert.deepEqual(
    tree.rows().map((row) => row.title),
    ["第一卷", "第一章", "第二章", "第三章", "第二卷"],
    "新建的章要直接出现在第一卷里，且卷是展开的",
  );
});

test("改名后**重拉这一层**：渲染标题由核心重算（号是位置的函数，会连带变）", async () => {
  const { transport, calls } = fakeWorld(BOOK);
  const tree = new DirectoryTree(transport);
  await tree.openWork(7);
  await tree.toggle(1);
  const before = calls.length;

  await tree.rename(2, "  第{$N}章 引子  ");
  // ① 写库；② 把这一层拉回来——改宏的有无会让**同层后面每一章的号**都变，
  //    就地只改一个节点的话，树上挂的还是旧渲染结果（真机反馈过）
  assert.deepEqual(calls.slice(before), ["rename:2:第{$N}章 引子", "children:7:1"]);
  const row = tree.rows().find((item) => item.id === 2);
  assert.equal(row?.title, "第{$N}章 引子", "原文照存（宏还在）");
  assert.equal(row?.title_rendered, "第1章 引子", "显示用的是核心渲染后的那一份");

  await tree.rename(2, "第{$N}章 引子");
  await tree.rename(2, "   ");
  assert.equal(calls.length, before + 2, "没变 / 空标题都不该再跑一趟");
});

test("同层拖动：索引要扣掉自己占的那一位", async () => {
  const { transport, calls } = fakeWorld(BOOK);
  const tree = new DirectoryTree(transport);
  await tree.openWork(7);
  await tree.toggle(1); // 第一卷：第一章(2) 第二章(3)

  // 把第一章拖到"第二章之后"：界面给的落点索引是 2，核心要的是 1
  await tree.move(2, 1, 2);
  assert.deepEqual(calls.slice(2), ["move:2:1:1", "children:7:-", "children:7:1"]);
  assert.deepEqual(
    tree.rows().map((row) => row.title),
    ["第一卷", "第二章", "第一章", "第二卷"],
  );
});

test("位置没变就不跑这一趟（免得白写一次库）", async () => {
  const { transport, calls } = fakeWorld(BOOK);
  const tree = new DirectoryTree(transport);
  await tree.openWork(7);
  await tree.toggle(1);
  const before = calls.length;

  await tree.move(3, 1, 1); // 已经在第二位
  assert.equal(calls.length, before, "原地放下不该触发任何写入");
});

test("跨层拖动：把章挪出卷，两侧都要重拉，并展开新父级", async () => {
  const { transport, calls } = fakeWorld(BOOK);
  const tree = new DirectoryTree(transport);
  await tree.openWork(7);
  await tree.toggle(1);

  await tree.move(2, null, 1); // 第一章挪到根级第二位（两卷之间）
  assert.deepEqual(calls.slice(2), ["move:2:-:1", "children:7:-", "children:7:1"]);
  assert.deepEqual(
    tree.rows().map((row) => row.title),
    ["第一卷", "第二章", "第一章", "第二卷"],
  );
});

test("不许把节点拖进自己的子树（拖了也当没拖）", async () => {
  const { transport, calls } = fakeWorld(BOOK);
  const tree = new DirectoryTree(transport);
  await tree.openWork(7);
  await tree.toggle(1);
  await tree.toggle(3); // 场景卡要先看得见，才可能被当成落点

  const before = calls.length;
  await tree.move(3, 4, 0); // 场景卡是第二章的孩子
  assert.equal(calls.length, before, "环不成立，连库都不该惊动");
});

test("重拉可见的层：根 + 已展开的每一层，展开状态不丢", async () => {
  const { transport, calls } = fakeWorld(BOOK);
  const tree = new DirectoryTree(transport);
  await tree.openWork(7);
  await tree.toggle(1);
  const before = calls.length;

  await tree.reloadVisible();
  assert.equal(calls.length, before + 2, "只重拉根与展开的那一层");
  assert.equal(tree.rows()[0].expanded, true, "重拉不该把展开状态冲掉");
  assert.deepEqual(
    tree.rows().map((row) => row.title),
    ["第一卷", "第一章", "第二章", "第二卷"],
  );
});

test("界面按「+」该干什么：能写正文的往后插，容器往里加", () => {
  // 章两样都占：作者的意图是"再来一章"，不是"往章里塞一章"
  assert.equal(addIntent({ holds_body: true, accepts_children: true }), "after");
  assert.equal(addIntent({ holds_body: false, accepts_children: true }), "inside");
  assert.equal(addIntent({ holds_body: true, accepts_children: false }), "after");
  assert.equal(addIntent({ holds_body: false, accepts_children: false }), null);
});

test("展开箭头跟着孩子走：新建后出现，搬空后消失", async () => {
  const { transport } = fakeWorld(BOOK);
  const tree = new DirectoryTree(transport);
  await tree.openWork(7);

  const empty = tree.rows().find((row) => row.title === "第二卷");
  assert.equal(empty?.has_children, false, "空卷一开始没有箭头");

  const created = await tree.create(5, "chapter", "第一章");
  assert.equal(tree.rows().find((row) => row.id === 5)?.has_children, true, "建进去就该有箭头");

  await tree.move(created, null, 0);
  assert.equal(tree.rows().find((row) => row.id === 5)?.has_children, false, "搬空了箭头要收起来");
});

test("定位到正在写的那一章：它所在的卷自动展开，章在收起的卷里也看得见", async () => {
  const { transport, calls } = fakeWorld(BOOK);
  const tree = new DirectoryTree(transport);
  await tree.openWork(7);
  assert.equal(
    tree.rows().some((row) => row.id === 4),
    false,
    "展开前，深层的场景卡看不见",
  );

  await tree.reveal(4); // 场景卡 → 第二章 → 第一卷
  assert.deepEqual(calls.slice(1), ["ancestors:4", "children:7:1", "children:7:3"]);
  assert.deepEqual(
    tree.rows().map((row) => `${row.depth}:${row.title}`),
    ["0:第一卷", "1:第一章", "1:第二章", "2:场景卡", "0:第二卷"],
    "这条路要一层层摊开，别的卷不受影响",
  );
});

test("反复定位同一条路：拉过的层不重拉（切章很频繁）", async () => {
  const { transport, calls } = fakeWorld(BOOK);
  const tree = new DirectoryTree(transport);
  await tree.openWork(7);
  await tree.reveal(4);
  const before = calls.length;

  await tree.reveal(4);
  assert.deepEqual(calls.slice(before), ["ancestors:4"], "只问一次祖先链，不再拉层");
});

test("根级的章：祖先链是空的，也不该白拉一层", async () => {
  const { transport, calls } = fakeWorld(BOOK);
  const tree = new DirectoryTree(transport);
  await tree.openWork(7);
  const before = calls.length;

  await tree.reveal(5); // 第二卷就在根级
  assert.deepEqual(calls.slice(before), ["ancestors:5"]);
});

test("还没打开作品就定位：什么都不做，也不去问核心", async () => {
  const { transport, calls } = fakeWorld(BOOK);
  const tree = new DirectoryTree(transport);
  await tree.reveal(4);
  assert.deepEqual(calls, []);
});

/** 容器行小字的夹具：四个字段一个都不能少（`TreeRow` 直接继承核心那份节点类型） */
const VOLUME_34K = {
  chapter_count: 12,
  subtree_word_count: 34000,
  subtree_char_count: 34000,
  subtree_chars_no_punct: 32000,
};
const EMPTY_VOLUME = {
  chapter_count: 0,
  subtree_word_count: 0,
  subtree_char_count: 0,
  subtree_chars_no_punct: 0,
};

test("容器行的小字：本卷几章 · 共多少字（设了卷长就是 x/y）", () => {
  // 口径跟树上章行一致：现在这棵树用「词」档
  assert.equal(containerLabel(VOLUME_34K, null, "words"), "12章 · 3.4万");
  assert.equal(containerLabel(VOLUME_34K, 30, "words"), "12/30章 · 3.4万");
  assert.equal(containerLabel(EMPTY_VOLUME, 30, "words"), "空", "空卷就说空");
  assert.equal(containerLabel({ ...EMPTY_VOLUME, subtree_word_count: 900 }, null, "words"), "0章 · 900",
    "只有卡片没有章时，字数照报");
});

test("容器行的合计跟着口径走——不然「206 字」旁边写着「200 词」", () => {
  const row = {
    chapter_count: 3,
    subtree_word_count: 200,
    subtree_char_count: 206,
    subtree_chars_no_punct: 180,
  };
  assert.equal(containerLabel(row, null, "chars"), "3章 · 206");
  assert.equal(containerLabel(row, null, "chars_no_punct"), "3章 · 180");
  assert.equal(containerLabel(row, null, "words"), "3章 · 200");
});

test("容器行的汇总来自核心：本卷几章、共多少字", async () => {
  const { transport } = fakeWorld(BOOK);
  const tree = new DirectoryTree(transport);
  await tree.openWork(7);

  const volume = tree.rows().find((row) => row.title === "第一卷");
  assert.equal(volume?.chapter_count, 2, "第一卷里两章");
  assert.equal(volume?.subtree_word_count, 2000, "1200 + 800（场景卡没字数）");
  assert.equal(volume?.chapter_count, 2, "场景卡不算章");
});

test("落盘改字数：各层容器的「共多少字」跟着挪，不重拉目录", async () => {
  const { transport, calls } = fakeWorld(BOOK);
  const tree = new DirectoryTree(transport);
  await tree.openWork(7);
  await tree.toggle(1);
  const before = calls.length;

  tree.applyCounts(2, { char_count: 1500, chars_no_punct: 1450, word_count: 1300 }, true);
  assert.equal(calls.length, before, "不该为几个字重拉目录");
  const volume = tree.rows().find((row) => row.title === "第一卷");
  // 卷那一行的小字：**三个口径的合计都要跟着差额走**（只修一个的话，换档位又不准了）
  assert.equal(volume?.subtree_char_count, 2300, "逐字（含标点）：2000 + 300");
  assert.equal(volume?.subtree_chars_no_punct, 2250, "逐字（不含标点）：2000 + 250");
  assert.equal(volume?.subtree_word_count, 2100, "按词：2000 + 100");
  // 行内三个口径一起更新——真机报过"状态栏 206 字、树上还写着 111"，
  // 就是因为这里只更新了 word_count，而树上显示哪个数由作者的档位决定
  const chapter = tree.rows().find((row) => row.id === 2);
  assert.equal(chapter?.char_count, 1500);
  assert.equal(chapter?.chars_no_punct, 1450);
  assert.equal(chapter?.word_count, 1300);
});

test("删掉一段：连子树一起走，并且重拉看得见的层", async () => {
  const { transport, calls } = fakeWorld(BOOK);
  const tree = new DirectoryTree(transport);
  await tree.openWork(7);
  await tree.toggle(1);
  await tree.toggle(3); // 把场景卡也露出来
  const before = calls.length;

  const removed = await tree.remove(3); // 删第二章：它的场景卡跟着走
  assert.equal(removed, 2, "连带子树一共两项");
  assert.deepEqual(calls.slice(before), ["remove:3", "children:7:-", "children:7:1"]);
  assert.deepEqual(
    tree.rows().map((row) => row.title),
    ["第一卷", "第一章", "第二卷"],
    "删掉的第二章与它的场景卡都不在目录里了",
  );
});

test("删之前先问一句：这段是不是我正在写的那一支", async () => {
  const { transport } = fakeWorld(BOOK);
  const tree = new DirectoryTree(transport);
  await tree.openWork(7);
  await tree.toggle(1);
  await tree.toggle(3);

  assert.equal(tree.contains(3, 4), true, "场景卡在第二章下面");
  assert.equal(tree.contains(3, 3), true, "它自己也算");
  assert.equal(tree.contains(3, 2), false, "兄弟不算");
  assert.equal(tree.contains(1, 4), true, "隔着两层的子孙也算");
});

test("落盘后更新这一行的字数：不重拉目录", async () => {
  const { transport, calls } = fakeWorld(BOOK);
  const tree = new DirectoryTree(transport);
  await tree.openWork(7);
  await tree.toggle(1);
  const before = calls.length;

  tree.applyCounts(2, { char_count: 1500, chars_no_punct: 1450, word_count: 1300 }, true);
  assert.equal(calls.length, before);
  const row = tree.rows()[1];
  assert.equal(row.char_count, 1500);
  assert.equal(row.chars_no_punct, 1450);
  assert.equal(row.word_count, 1300);
});

test("没加载过的那一层：落盘更新碰不到它，下次重拉自然就对", async () => {
  const { transport, calls } = fakeWorld(BOOK);
  const tree = new DirectoryTree(transport);
  await tree.openWork(7); // 只拉了根那一层，第一章还没进树

  tree.applyCounts(2, { char_count: 9999, chars_no_punct: 9999, word_count: 9999 }, true);
  assert.equal(calls.length, 1, "不该为了更新它去多拉一层");

  await tree.toggle(1); // 展开时按库里的数字进来，不会带着上面那笔假数据
  assert.equal(tree.rows().find((row) => row.id === 2)?.char_count, 1200);
});

test("换一部作品：旧树清空，展开状态不串", async () => {
  const { transport, calls } = fakeWorld(BOOK);
  const tree = new DirectoryTree(transport);
  await tree.openWork(7);
  await tree.toggle(1);

  await tree.openWork(8);
  assert.deepEqual(calls.slice(-1), ["children:8:-"]);
  assert.deepEqual(
    tree.rows().map((row) => row.title),
    ["第一卷", "第二卷"],
    "换作品后旧书的展开状态与节点都不该留着",
  );
});

test("还没打开作品就动手：明确报错，不是静默什么都不做", async () => {
  const { transport } = fakeWorld(BOOK);
  const tree = new DirectoryTree(transport);
  await assert.rejects(() => tree.create(null, "volume", "第一卷"), /还没有打开作品/);
});
