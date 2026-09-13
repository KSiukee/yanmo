// 「一句话」的测试：**存下了才改显示、切章以核心给的为准**。
//
// 这里测的是"设备"而不是"渲染"：给一个替身传输层，看它在各种情况下叫了哪些动作、显示跟着谁走。

import { test } from "node:test";
import assert from "node:assert/strict";
import { ref } from "vue";

import { useChapterNote, type NoteTransport } from "./note.ts";

function build(options: { fail?: boolean } = {}) {
  const calls: string[] = [];
  const errors: string[] = [];
  const transport: NoteTransport = {
    save: async (node_id, text) => {
      calls.push(`save:${node_id}:${text}`);
      if (options.fail) throw new Error("存不下去");
    },
  };
  const note = useChapterNote({
    transport,
    nodeId: ref<number | null>(7),
    onError: (message) => errors.push(message),
  });
  return { note, calls, errors };
}

test("点开：拿存下那份起头，存下了才改显示", async () => {
  const { note, calls } = build();
  note.reset("他推开门。");
  assert.equal(note.value.value, "他推开门。");
  assert.equal(note.editing.value, false);

  note.open();
  assert.equal(note.editing.value, true);
  assert.equal(note.draft.value, "他推开门。", "编辑从存下那份起头");

  note.draft.value = "他推开门，屋里没有人。";
  await note.save();
  assert.deepEqual(calls, ["save:7:他推开门，屋里没有人。"]);
  assert.equal(note.value.value, "他推开门，屋里没有人。");
  assert.equal(note.editing.value, false, "存下了就把输入框收起来");
});

test("存不下去：显示不动、错误交出去、框留着", async () => {
  const { note, calls, errors } = build({ fail: true });
  note.reset("");
  note.open();
  note.draft.value = "写了但存不下";
  await note.save();
  assert.deepEqual(calls, ["save:7:写了但存不下"]);
  assert.equal(note.value.value, "", "没存下就不能装作写好了");
  assert.equal(note.editing.value, true, "框要留着，别让作者白写");
  assert.deepEqual(errors, ["存不下去"]);
});

test("切章：以核心给的为准，没存的草稿跟着丢掉", () => {
  const { note } = build();
  note.reset("第一章的一句话");
  note.open();
  note.draft.value = "改了但没存";
  note.reset("第二章的一句话");
  assert.equal(note.value.value, "第二章的一句话");
  assert.equal(note.draft.value, "第二章的一句话");
  assert.equal(note.editing.value, false);
});
