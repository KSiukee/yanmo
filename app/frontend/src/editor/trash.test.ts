// 回收站的测试：**冲突要交给作者拿主意**，系统不替他改名、也不悄悄恢复出两章同名。
//
// 这里测的是"设备"而不是"渲染"：给一个替身传输层，看它在各种选择下到底叫了哪些动作。

import { test } from "node:test";
import assert from "node:assert/strict";
import { ref } from "vue";

import type { NameClash, RestorePreview, TrashEntry } from "../api/core.ts";
import { trashLabel, useTrash, type TrashTransport } from "./trash.ts";

function entry(id: number, title: string, kind = "node"): TrashEntry {
  return {
    kind,
    id,
    title,
    work_id: 1,
    work_title: "长夜",
    deleted_at: 0,
    nodes: 1,
  };
}

function preview(clashes: NameClash[]): RestorePreview {
  return {
    work_id: 1,
    work_title: "长夜",
    parent_title: "第一卷",
    index: 2,
    name_clashes: clashes,
  };
}

function build(initial: TrashEntry[], clashes: Array<{ id: number; title: string; word_count: number }> = []) {
  const calls: string[] = [];
  let entries = [...initial];
  const transport: TrashTransport = {
    list: async () => entries,
    restoreWork: async (work_id) => {
      calls.push(`restore-work:${work_id}`);
      entries = entries.filter((item) => !(item.kind === "work" && item.id === work_id));
      return 1;
    },
    preview: async (node_id) => {
      calls.push(`preview:${node_id}`);
      // 替身只关心这几个数：三口径给同一份，免得每个调用点都要写三遍
      return preview(clashes.map((c) => ({ ...c, char_count: c.word_count, chars_no_punct: c.word_count })));
    },
    restoreNode: async (node_id, title) => {
      calls.push(`restore-node:${node_id}:${title ?? "-"}`);
      entries = entries.filter((item) => item.id !== node_id);
      return 1;
    },
    purgeWork: async (work_id) => {
      calls.push(`purge-work:${work_id}`);
      return 1;
    },
    purgeNode: async (node_id) => {
      calls.push(`purge-node:${node_id}`);
      return 1;
    },
    empty: async () => {
      calls.push("empty");
      entries = [];
      return 1;
    },
  };
  const trash = useTrash({
    transport,
    workId: ref(1),
    reopen: async () => {
      calls.push("reopen");
    },
  });
  return { trash, calls };
}

test("回收站那一行怎么念", () => {
  assert.equal(trashLabel(entry(1, "第19章")), "《长夜》· 第19章");
  assert.equal(trashLabel({ ...entry(1, "第一卷"), nodes: 4 }), "《长夜》· 第一卷（连带 3 项）");
  assert.equal(trashLabel(entry(9, "长夜", "work")), "整本《长夜》");
});

test("没冲突就直接恢复：不给正常路径多问一句", async () => {
  const { trash, calls } = build([entry(5, "第19章")]);
  await trash.restore(entry(5, "第19章"));
  assert.deepEqual(calls, ["preview:5", "restore-node:5:-"]);
  assert.equal(trash.conflict.value, null);
});

test("同名冲突：**先不恢复**，把决定权交给作者", async () => {
  const { trash, calls } = build([entry(5, "第19章")], [{ id: 21, title: "第19章", word_count: 800 }]);
  const note = await trash.restore(entry(5, "第19章"));

  assert.deepEqual(calls, ["preview:5"], "只预检，什么都没动");
  assert.equal(note, "");
  assert.ok(trash.conflict.value, "冲突挂在那儿等作者选");
  assert.equal(trash.conflict.value?.preview.name_clashes[0].word_count, 800);
});

test("冲突下选「照原样恢复」：恢复时不带新名字", async () => {
  const { trash, calls } = build([entry(5, "第19章")], [{ id: 21, title: "第19章", word_count: 800 }]);
  await trash.restore(entry(5, "第19章"));
  await trash.resolveConflict(null);
  assert.deepEqual(calls, ["preview:5", "restore-node:5:-"]);
  assert.equal(trash.conflict.value, null);
});

test("冲突下选「恢复并改名」：名字原样传给核心（由作者给）", async () => {
  const { trash, calls } = build([entry(5, "第19章")], [{ id: 21, title: "第19章", word_count: 800 }]);
  await trash.restore(entry(5, "第19章"));
  await trash.resolveConflict("第19章（旧稿）");
  assert.deepEqual(calls, ["preview:5", "restore-node:5:第19章（旧稿）"]);
});

test("冲突下选「取消」：一个动作都不发", async () => {
  const { trash, calls } = build([entry(5, "第19章")], [{ id: 21, title: "第19章", word_count: 800 }]);
  await trash.restore(entry(5, "第19章"));
  trash.cancelConflict();
  assert.deepEqual(calls, ["preview:5"]);
  assert.equal(trash.conflict.value, null);
});

test("删掉正在写的那本书之后再彻底删：要给自己留个落点", async () => {
  const { trash, calls } = build([entry(1, "长夜", "work")]);
  await trash.purge(entry(1, "长夜", "work"));
  assert.deepEqual(calls, ["purge-work:1", "reopen"]);
});

test("删的不是当前这本：不用换地方", async () => {
  const { trash, calls } = build([entry(2, "短歌", "work")]);
  await trash.purge(entry(2, "短歌", "work"));
  assert.deepEqual(calls, ["purge-work:2"]);
});
