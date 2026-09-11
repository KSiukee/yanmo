// 章节切换的测试：**顺序、不丢字、失败停在原地**——三条都是"不丢稿"的落地。
//
// 用替身记账（假控制器 + 假命令），不碰真核心。注意账本只有一本：
// 控制器的 flush 与切换动作写进同一个数组，顺序才看得准。

import { test } from "node:test";
import assert from "node:assert/strict";

import type { EditorCursor, EditorSnapshot } from "../api/core.ts";
import type { Autosave } from "./autosave.ts";
import { ChapterSwitch } from "./chapters.ts";

function snapshot(node_id: number, body = "正文"): EditorSnapshot {
  return {
    work_id: 1,
    node_id,
    title: `第${node_id}章`,
    body,
    char_count: body.length,
    word_count: body.length,
    fingerprint: `fp(${body})`,
    cursor: { anchor: 0, head: 0, scroll_top: 0 },
  };
}

interface Setup {
  /** 当前挂着的章（null = 还没挂） */
  current?: { node_id: number; failFlush?: boolean } | null;
  cursor?: EditorCursor | null;
  failLoad?: number;
}

function build(options: Setup = {}) {
  const calls: string[] = [];
  const errors: string[] = [];
  const current = options.current ?? null;

  const autosave = current
    ? ({
        node_id: current.node_id,
        flush: async () => {
          calls.push(`flush:${current.node_id}`);
          if (current.failFlush) throw new Error("磁盘写入失败");
        },
      } as unknown as Autosave)
    : null;

  const switchTo = new ChapterSwitch({
    autosave: () => autosave,
    currentCursor: () => options.cursor ?? null,
    saveCursor: async (node_id, cursor) => {
      calls.push(`cursor:${node_id}:${cursor.anchor}`);
    },
    loadChapter: async (node_id) => {
      calls.push(`load:${node_id}`);
      if (options.failLoad === node_id) throw new Error("这一章被删了");
      return snapshot(node_id);
    },
    applyChapter: (chapter) => {
      calls.push(`apply:${chapter.node_id}`);
    },
    startAutosave: (chapter) => {
      calls.push(`autosave:${chapter.node_id}`);
    },
    onError: (message) => {
      errors.push(message);
    },
  });
  return { switchTo, calls, errors };
}

test("切章顺序：先落盘、记光标，再取新章、换内容、换控制器", async () => {
  const { switchTo, calls } = build({
    current: { node_id: 7 },
    cursor: { anchor: 42, head: 42, scroll_top: 900 },
  });

  assert.equal(await switchTo.to(8), "switched");
  assert.deepEqual(calls, ["flush:7", "cursor:7:42", "load:8", "apply:8", "autosave:8"]);
});

test("落盘失败就不许切：停在原章，一个字都不动", async () => {
  const { switchTo, calls, errors } = build({ current: { node_id: 7, failFlush: true } });

  assert.equal(await switchTo.to(8), "blocked");
  assert.deepEqual(calls, ["flush:7"], "落盘没成功，连新章都不该去取");
  assert.match(errors[0], /磁盘写入失败/);
});

test("目标章取不到（被删了）：停在原章，不换内容也不换控制器", async () => {
  const { switchTo, calls, errors } = build({ current: { node_id: 7 }, failLoad: 8 });

  assert.equal(await switchTo.to(8), "blocked");
  assert.deepEqual(calls, ["flush:7", "load:8"], "取不到就到此为止");
  assert.match(errors[0], /被删了/);
});

test("连点下一章：切换期间的请求直接忽略", async () => {
  const { switchTo } = build({ current: { node_id: 7 } });

  const first = switchTo.to(8);
  const second = await switchTo.to(9);
  assert.equal(second, "ignored", "正在切的时候再点，不该排队切两章");
  assert.equal(await first, "switched");
});

test("切到当前这一章：忽略（不浪费一次落盘往返）", async () => {
  const { switchTo, calls } = build({ current: { node_id: 7 } });
  assert.equal(await switchTo.to(7), "ignored");
  assert.deepEqual(calls, []);
});

test("直接给一章快照（新建路径）：照样先落盘、再换内容换控制器，不去取章", async () => {
  const { switchTo, calls } = build({ current: { node_id: 7 } });
  assert.equal(await switchTo.to(snapshot(9, "新章正文")), "switched");
  assert.deepEqual(calls, ["flush:7", "apply:9", "autosave:9"]);
});

test("还没挂章时（首次载入）直接载入目标章", async () => {
  const { switchTo, calls } = build({ current: null });
  assert.equal(await switchTo.to(3), "switched");
  assert.deepEqual(calls, ["load:3", "apply:3", "autosave:3"]);
});
