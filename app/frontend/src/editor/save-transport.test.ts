// 落盘传输：**来源跟着落盘成败走**——成功才清、失败放回去。
import { test } from "node:test";
import assert from "node:assert/strict";

import { saveTransport } from "./save-transport.ts";
import { WriteOrigins } from "./write-origin.ts";
import type { SaveOrigin, SourceBreakdown } from "../api/source.ts";

const SOURCES: SourceBreakdown = {
  shares: [{ source: "pasted", char_count: 12, chars_no_punct: 10, word_count: 5 }],
};

const ACK = {
  char_count: 12,
  chars_no_punct: 10,
  word_count: 5,
  fingerprint: "fp",
  sources: SOURCES,
};

test("落盘成功：观测交给核心，来源分布交回界面，观测清空", async () => {
  const origins = new WriteOrigins();
  origins.notePaste(12);
  let seen: SaveOrigin | undefined;
  let received: SourceBreakdown | null = null;

  const transport = saveTransport({
    origins,
    tz: () => 480,
    onSources: (next) => {
      received = next;
    },
    fingerprint: async () => "fp",
    emergency: async () => ACK,
    save: async (_node, _body, tz, origin) => {
      assert.equal(tz, 480, "时区每次现取");
      seen = origin;
      return ACK;
    },
  });

  const ack = await transport.save(7, "粘进来的字");
  assert.equal(seen?.kind, "pasted");
  assert.equal(seen?.clipboard_chars, 12);
  assert.deepEqual(received, SOURCES);
  assert.equal(origins.take(), null, "成功之后观测已清空");
  assert.deepEqual(ack, ACK);
});

test("落盘失败：观测放回去（否则重试时那一千字会被记成未记录）", async () => {
  const origins = new WriteOrigins();
  origins.notePaste(1000);
  const transport = saveTransport({
    origins,
    tz: () => 0,
    onSources: () => assert.fail("失败时不该刷来源"),
    fingerprint: async () => "fp",
    emergency: async () => ACK,
    save: async () => {
      throw new Error("盘满了");
    },
  });

  await assert.rejects(() => transport.save(1, "一大段"), /盘满/);
  assert.deepEqual(origins.take(), { kind: "pasted", clipboard_chars: 1000 });
});

test("抢救（emergency）也走同一条：成功才清、失败放回去", async () => {
  const origins = new WriteOrigins();
  origins.note("typed");
  let calls = 0;
  const transport = saveTransport({
    origins,
    tz: () => 0,
    onSources: () => {},
    save: async () => ACK,
    fingerprint: async () => "fp",
    emergency: async () => {
      calls += 1;
      if (calls === 1) throw new Error("抢救也没成");
      return ACK;
    },
  });

  await assert.rejects(() => transport.emergency(1, "手上的字", "desync"));
  assert.deepEqual(origins.take(), { kind: "typed" }, "抢救失败也要把观测放回去");

  origins.note("typed");
  await transport.emergency(1, "手上的字", "desync");
  assert.equal(origins.take(), null);
});

test("一次落盘里没观测到任何来源：原样交给核心（不替它编一个）", async () => {
  const origins = new WriteOrigins();
  let seen: SaveOrigin | undefined | null = null;
  const transport = saveTransport({
    origins,
    tz: () => 0,
    onSources: () => {},
    fingerprint: async () => "fp",
    emergency: async () => ACK,
    save: async (_node, _body, _tz, origin) => {
      seen = origin;
      return ACK;
    },
  });
  await transport.save(1, "程序改的字");
  assert.equal(seen, undefined, "没观测到就不传 origin");
});
