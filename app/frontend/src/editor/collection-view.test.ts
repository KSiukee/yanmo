// 集视图状态机的验收：**一键排序真的写回、顺序没变就一次写都不发生**。
//
// 这一份只注入替身命令（不碰界面也不碰核心）：钉的是「算出来的顺序必须有去处」——
// 排完要走重排那条命令、屏上那份换成回读的那一屏；本来就是这个顺序时别白跑一趟。

import { test } from "node:test";
import assert from "node:assert/strict";

import type { ArticleEntry, CollectionView } from "../api/collection";
import { useCollection, type CollectionTransport } from "./collection-view.ts";
import { t } from "../locales/index.ts";

function entry(id: number, created_at: number, classify: { genre?: string; tags?: string } = {}): ArticleEntry {
  return {
    id,
    title: `piece ${id}`,
    status: "draft",
    word_count: 10,
    char_count: 100,
    chars_no_punct: 90,
    created_at,
    updated_at: 0,
    genre: classify.genre ?? "",
    tags: classify.tags ?? "",
  };
}

function view(entries: ArticleEntry[]): CollectionView {
  return {
    id: 7,
    title: "anthology",
    entries,
    target_pieces: null,
    target_words: null,
    pieces: entries.length,
    words: 0,
    chars: 0,
    chars_no_punct: 0,
  };
}

/** 一个只认真办事的替身：把「重排」收到的顺序记下来，并按它回一屏。 */
function transportOf(start: ArticleEntry[]): {
  reorders: number[][];
  transport: CollectionTransport;
} {
  const reorders: number[][] = [];
  const byId = new Map(start.map((item) => [item.id, item]));
  const pick = (ids: number[]) => ids.map((id) => byId.get(id) as ArticleEntry);
  const transport: CollectionTransport = {
    view: () => Promise.resolve(view(start)),
    add: () => Promise.resolve(view(start)),
    remove: () => Promise.resolve(view(start)),
    reorder: (_work_id, ids) => {
      reorders.push(ids);
      return Promise.resolve(view(pick(ids)));
    },
    setTarget: () => Promise.resolve(view(start)),
    create: () => Promise.resolve(view(start)),
    loose: () => Promise.resolve([]),
  };
  return { reorders, transport };
}

test("一键排序：顺序变了就写回，屏上换成回读的那一屏", async () => {
  const first = entry(1, 100);
  const second = entry(2, 200);
  const { reorders, transport } = transportOf([second, first]);
  const state = useCollection({ transport });
  await state.enter(7);

  await state.sort("written", "chars");

  assert.deepEqual(reorders, [[1, 2]], "按写成先后：早的那篇到前面，走的正是重排这条命令");
  assert.deepEqual(state.view.value?.entries.map((item) => item.id), [1, 2], "屏上那份是回读回来的");
  assert.equal(state.note.value, t("collection.note_sorted"), "排完给一句话交代");
});

test("一键排序：顺序没变就一次写都不发生", async () => {
  const first = entry(1, 100);
  const second = entry(2, 200);
  const { reorders, transport } = transportOf([first, second]);
  const state = useCollection({ transport });
  await state.enter(7);

  await state.sort("written", "chars");

  assert.deepEqual(reorders, [], "本来就是这个顺序，不该去写库");
  assert.equal(state.note.value, "", "也不该在屏上留一句「顺序排好了」");
});

test("一键排序：没开着集的时候什么都不做", async () => {
  const { reorders, transport } = transportOf([entry(1, 1)]);
  const state = useCollection({ transport });

  await state.sort("length", "chars");

  assert.deepEqual(reorders, [], "没有集就没有可排的东西");
});

test("第三档「按主题」也走同一条写回的路（算出来的顺序有去处）", async () => {
  // 现在是「悬疑、科幻、空」——按主题排完应当是「悬疑、科幻、空」，所以这里换个起点：
  // 「科幻、空、悬疑」→「悬疑、科幻、空」（组间按体裁名字面量序）
  const scifi = entry(2, 200, { genre: "科幻", tags: "太空" });
  const blank = entry(3, 300);
  const mystery = entry(1, 100, { genre: "悬疑", tags: "密室" });
  const { reorders, transport } = transportOf([scifi, blank, mystery]);
  const state = useCollection({ transport });
  await state.enter(7);

  await state.sort("theme", "chars");

  assert.deepEqual(reorders, [[1, 2, 3]], "同类挨着、空的垫底，顺序真的写回");
  assert.deepEqual(state.view.value?.entries.map((item) => item.id), [1, 2, 3]);
  assert.equal(state.note.value, t("collection.note_sorted"));
});
