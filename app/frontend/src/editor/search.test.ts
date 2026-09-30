// 正文检索的测试：**防抖只发一趟、过时的响应不许盖新的、空问题不发车**。
//
// 测的是"设备"而不是"渲染"：给一个替身传输层与一只假计时器，看它到底在什么时候
// 发了哪一趟、把什么摆到结果里。假计时器替掉了真的 250ms——不必等。

import { test } from "node:test";
import assert from "node:assert/strict";

import type { SearchHit } from "../api/core.ts";
import { highlight, SEARCH_LIMIT, useSearch, type SearchTransport } from "./search.ts";

function hit(node_id: number, snippet = "……长夜将至，四周安静得能听见雪落。"): SearchHit {
  return {
    node_id,
    work_id: 1,
    work_title: "长夜",
    title: "第 1 章",
    snippet,
    matched_title: false,
  };
}

/** 假计时器：把"过一会儿"攥在手里，测试自己拨过去。 */
function fakeTimer() {
  const pending = new Map<number, () => void>();
  let next = 1;
  return {
    schedule: (fn: () => void) => {
      const handle = next;
      next += 1;
      pending.set(handle, fn);
      return handle as unknown as ReturnType<typeof setTimeout>;
    },
    cancel: (handle: ReturnType<typeof setTimeout>) => {
      pending.delete(handle as unknown as number);
    },
    /** 到点了：把排着的那几次都跑掉（防抖顶掉的已经被 cancel 拿走了） */
    flush: () => {
      const fns = [...pending.values()];
      pending.clear();
      for (const fn of fns) fn();
    },
    size: () => pending.size,
  };
}

/** 让在飞的那一趟把 then 之后的代码跑完（传输层只回一个已经 resolved 的 promise）。 */
const settle = () => new Promise<void>((resolve) => setImmediate(resolve));

test("防抖窗口内连打三个字：只发一趟，用的是最后那串", async () => {
  const calls: Array<[string, number | null, number]> = [];
  const transport: SearchTransport = async (query, work_id, limit) => {
    calls.push([query, work_id, limit]);
    return [hit(7)];
  };
  const timer = fakeTimer();
  const search = useSearch({
    transport,
    openHit: async () => {},
    schedule: timer.schedule,
    cancel: timer.cancel,
  });

  search.type("长");
  search.type("长夜");
  search.type("长夜将");
  assert.equal(timer.size(), 1, "三次击键只留最后一次防抖");
  assert.deepEqual(calls, [], "还没到点，一趟都不该发");

  timer.flush();
  await settle();
  assert.deepEqual(calls, [["长夜将", null, SEARCH_LIMIT]]);
  assert.equal(search.hits.value.length, 1);
  assert.equal(search.busy.value, false);
});

test("慢的旧响应后到：不许盖掉新结果（epoch）", async () => {
  const calls: string[] = [];
  let releaseOld: ((hits: SearchHit[]) => void) | null = null;
  const transport: SearchTransport = (query) => {
    calls.push(query);
    if (query === "旧问题") {
      return new Promise<SearchHit[]>((resolve) => {
        releaseOld = resolve;
      });
    }
    return Promise.resolve([hit(2, "……新结果……")]);
  };
  const timer = fakeTimer();
  const search = useSearch({
    transport,
    openHit: async () => {},
    schedule: timer.schedule,
    cancel: timer.cancel,
  });

  search.type("旧问题");
  timer.flush(); // 第一趟在飞（故意不回）
  search.type("新问题");
  timer.flush();
  await settle();
  assert.deepEqual(calls, ["旧问题", "新问题"]);
  assert.equal(search.hits.value[0].node_id, 2, "先摆上的是新那一趟的答案");

  assert.ok(releaseOld, "旧那一趟得真的在飞");
  releaseOld([hit(1, "……旧结果……")]);
  await settle();
  assert.equal(search.hits.value.length, 1);
  assert.equal(search.hits.value[0].node_id, 2, "过时的那一趟回来时不许改结果");
  assert.equal(search.busy.value, false);
});

test("空问题：一趟都不发，结果清空", async () => {
  const calls: string[] = [];
  const transport: SearchTransport = async (query) => {
    calls.push(query);
    return [hit(3)];
  };
  const timer = fakeTimer();
  const search = useSearch({
    transport,
    openHit: async () => {},
    schedule: timer.schedule,
    cancel: timer.cancel,
  });

  search.type("长夜");
  timer.flush();
  await settle();
  assert.equal(search.hits.value.length, 1);

  search.type("   ");
  assert.equal(timer.size(), 0, "空白不排防抖");
  assert.equal(search.hits.value.length, 0, "结果清空");
  assert.deepEqual(calls, ["长夜"], "只有第一次那一趟");
});

test("传输层抛错：error 有值、结果清空、不往外抛", async () => {
  const reported: string[] = [];
  const timer = fakeTimer();
  const search = useSearch({
    transport: async () => {
      throw new Error("boom");
    },
    openHit: async () => {},
    report: (detail) => reported.push(detail),
    schedule: timer.schedule,
    cancel: timer.cancel,
  });

  search.type("长夜");
  await search.run(); // 回车那条路：不等防抖，直接发
  assert.equal(search.error.value, "boom");
  assert.deepEqual(reported, ["boom"], "详情也交回调用方（由它组织句子）");
  assert.equal(search.hits.value.length, 0);
  assert.equal(search.busy.value, false);
});

test("点一条结果：openHit 恰好一次，弹窗收起来", async () => {
  const opened: number[] = [];
  const search = useSearch({
    transport: async () => [],
    openHit: async (one) => {
      opened.push(one.node_id);
    },
  });

  search.toggle();
  assert.equal(search.visible.value, true);
  await search.open(hit(7));
  assert.deepEqual(opened, [7]);
  assert.equal(search.visible.value, false);
});

test("命中够到上限：如实说「只显示前 N 条」", async () => {
  const full = Array.from({ length: SEARCH_LIMIT }, (_, index) => hit(index + 1));
  const search = useSearch({
    transport: async () => full,
    openHit: async () => {},
  });
  search.type("长夜");
  await search.run();
  assert.equal(search.hits.value.length, SEARCH_LIMIT);
  assert.equal(search.truncated.value, true);
});

test("高亮：命中在中间 / 没命中 / 空 query / 多处只标第一处，且拼回去逐字相同", () => {
  const cases: Array<[string, string, number[]]> = [
    // 片段、问题、期望被标出来的那几段（按 text 的下标）
    ["……长夜将至，四周安静。", "长夜", [2]],
    ["……长夜将至，长夜很长。", "长夜", [2]],
    ["……长夜将至。", "没有这一串", []],
    ["……长夜将至。", "   ", []],
    ["……The night is long.", "the", [2]],
    ["长夜", "长夜", [0]],
  ];
  for (const [snippet, query, expected] of cases) {
    const parts = highlight(snippet, query);
    assert.equal(
      parts.map((part) => part.text).join(""),
      snippet,
      `拼回去必须与原文逐字相同：${snippet} / ${query}`,
    );
    assert.deepEqual(
      parts.filter((part) => part.hit).map((part) => snippet.indexOf(part.text)),
      expected,
      `标出来的位置不对：${snippet} / ${query}`,
    );
  }

  // 多处出现：只标第一处（不是把整句标满）
  const parts = highlight("长夜将至，长夜无眠。", "长夜");
  assert.deepEqual(
    parts.map((part) => (part.hit ? `[${part.text}]` : part.text)).join(""),
    "[长夜]将至，长夜无眠。",
  );
});
