// 壳的写入观测：**没看见就不报、混了就记混、没落成要放回去**。
import { test } from "node:test";
import assert from "node:assert/strict";

import { isTypingKey, WriteOrigins } from "./write-origin.ts";

test("什么都没看见 → 不报（由核心记「未记录」，绝不当成打字）", () => {
  const origins = new WriteOrigins();
  assert.equal(origins.take(), null);
});

test("敲了键就是 typed；取走之后下一笔重新开始", () => {
  const origins = new WriteOrigins();
  origins.note("typed");
  assert.deepEqual(origins.take(), { kind: "typed" });
  assert.equal(origins.take(), null);
});

test("粘贴带剪贴板字数（只作证据）；与键入同在一笔 → 混合", () => {
  const origins = new WriteOrigins();
  origins.notePaste(42);
  assert.deepEqual(origins.take(), { kind: "pasted", clipboard_chars: 42 });

  origins.note("typed");
  origins.notePaste(7);
  assert.deepEqual(origins.take(), { kind: "mixed", clipboard_chars: 7 }, "混了就说混了，不猜各占多少");
});

test("落盘失败把观测放回去；下一笔连同它的字一起记", () => {
  const origins = new WriteOrigins();
  origins.notePaste(120);
  const taken = origins.take();
  origins.restore(taken);
  assert.deepEqual(origins.take(), { kind: "pasted", clipboard_chars: 120 });

  // 放回去之后作者又敲了几个字：这一笔确实是混的
  origins.restore({ kind: "pasted", clipboard_chars: 5 });
  origins.note("typed");
  assert.deepEqual(origins.take(), { kind: "mixed", clipboard_chars: 5 });
});

test("放回一个已经混合的观测，仍然是混合（分不出原来有哪几种了）", () => {
  const origins = new WriteOrigins();
  origins.restore({ kind: "mixed", clipboard_chars: 3 });
  assert.deepEqual(origins.take(), { kind: "mixed", clipboard_chars: 3 });
});

test("撤销一条记错的观测（插入被拒绝时什么也没发生）", () => {
  const origins = new WriteOrigins();
  origins.note("system");
  origins.forget("system");
  assert.equal(origins.take(), null);
});

test("isTypingKey：落字的键才算，命令键与组字态都不算", () => {
  const base = { ctrlKey: false, metaKey: false, altKey: false, isComposing: false };
  assert.equal(isTypingKey({ ...base, key: "a" }), true);
  assert.equal(isTypingKey({ ...base, key: "中" }), true);
  assert.equal(isTypingKey({ ...base, key: " " }), true);
  assert.equal(isTypingKey({ ...base, key: "Enter" }), false, "回车只加段落分隔");
  assert.equal(isTypingKey({ ...base, key: "Backspace" }), false, "删除不落字");
  assert.equal(isTypingKey({ ...base, key: "a", ctrlKey: true }), false, "Ctrl+A 是命令");
  assert.equal(isTypingKey({ ...base, key: "v", metaKey: true }), false);
  assert.equal(isTypingKey({ ...base, key: "Process", isComposing: true }), false, "组字走 compositionstart");
});
