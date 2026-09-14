// 快捷键匹配的验收：**认哪些键、绝不认哪些键**。
//
// 这张表是"别抢作者的键"的机械拦法——真机上出过的毛病（输入法组字时换章、
// 在输入框里打字被功能键打断）都在下面钉住。

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  SHORTCUTS,
  isTextFieldTarget,
  matchShortcut,
  shortcutKeys,
  type KeyLike,
  type MatchContext,
} from "./shortcuts.ts";

function key(overrides: Partial<KeyLike> & { key: string }): KeyLike {
  return {
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    metaKey: false,
    ...overrides,
  };
}

function ctx(overrides: Partial<MatchContext> = {}): MatchContext {
  return { dialogOpen: false, inTextField: false, zenOn: false, fullscreenOn: false, ...overrides };
}

test("命中的键：每一个都在表里认得出来", () => {
  assert.equal(matchShortcut(key({ key: "F11" }), ctx()), "fullscreen");
  assert.equal(matchShortcut(key({ key: "F9" }), ctx()), "zen");
  assert.equal(matchShortcut(key({ key: "s", ctrlKey: true }), ctx()), "save-now");
  assert.equal(matchShortcut(key({ key: "n", ctrlKey: true }), ctx()), "new-chapter");
  assert.equal(matchShortcut(key({ key: ",", ctrlKey: true }), ctx()), "settings");
  assert.equal(matchShortcut(key({ key: "b", ctrlKey: true }), ctx()), "shelf");
  assert.equal(
    matchShortcut(key({ key: "ArrowLeft", ctrlKey: true, altKey: true }), ctx()),
    "prev-chapter",
  );
  assert.equal(
    matchShortcut(key({ key: "ArrowRight", ctrlKey: true, altKey: true }), ctx()),
    "next-chapter",
  );
});

test("大写 / CapsLock 不影响命中（真机上最容易出现「按了没反应」）", () => {
  assert.equal(matchShortcut(key({ key: "S", ctrlKey: true }), ctx()), "save-now");
  assert.equal(matchShortcut(key({ key: "N", ctrlKey: true }), ctx()), "new-chapter");
  assert.equal(matchShortcut(key({ key: "B", ctrlKey: true }), ctx()), "shelf");
});

test("可打印键绝不劫持：单个字母 / 数字一律不认", () => {
  for (const plain of ["s", "n", "b", "a", "1", ",", "ArrowLeft", "Escape"]) {
    assert.equal(matchShortcut(key({ key: plain }), ctx()), null, `单键 ${plain} 不该被认走`);
  }
});

test("修饰键对不上就不认（多按少按都不认）", () => {
  // Ctrl+S 认；Ctrl+Shift+S / Ctrl+Alt+S / 光按 S 都不认
  assert.equal(matchShortcut(key({ key: "s", ctrlKey: true, shiftKey: true }), ctx()), null);
  assert.equal(matchShortcut(key({ key: "s", ctrlKey: true, altKey: true }), ctx()), null);
  assert.equal(matchShortcut(key({ key: "s" }), ctx()), null);
  // 方向键必须有 Ctrl+Alt 才算换章（防止误触把正在写的章切走）
  assert.equal(matchShortcut(key({ key: "ArrowRight" }), ctx()), null);
  assert.equal(matchShortcut(key({ key: "ArrowRight", ctrlKey: true }), ctx()), null);
});

test("输入法组字中：一律不认（组字时的字母与 Ctrl 是给输入法用的）", () => {
  const composing = { isComposing: true };
  assert.equal(matchShortcut(key({ key: "F9", ...composing }), ctx()), null);
  assert.equal(matchShortcut(key({ key: "F11", ...composing }), ctx()), null);
  assert.equal(matchShortcut(key({ key: "s", ctrlKey: true, ...composing }), ctx()), null);
  assert.equal(matchShortcut(key({ key: "Escape", ...composing }), ctx({ zenOn: true })), null);
});

test("长按不连发：repeat 一律不认", () => {
  assert.equal(matchShortcut(key({ key: "F9", repeat: true }), ctx()), null);
  assert.equal(
    matchShortcut(key({ key: "ArrowRight", ctrlKey: true, altKey: true, repeat: true }), ctx()),
    null,
  );
});

test("弹窗开着：一律不认（含 Esc——Esc 归弹窗自己）", () => {
  const open = ctx({ dialogOpen: true, zenOn: true });
  assert.equal(matchShortcut(key({ key: "s", ctrlKey: true }), open), null);
  assert.equal(matchShortcut(key({ key: "F9" }), open), null);
  assert.equal(matchShortcut(key({ key: "Escape" }), open), null, "一下 Esc 不能既关弹窗又退专注");
});

test("焦点在输入框里：功能键不认，带修饰键的组合照认", () => {
  const typing = ctx({ inTextField: true });
  assert.equal(matchShortcut(key({ key: "F9" }), typing), null, "在搜索框里按 F9 不该换布局");
  assert.equal(matchShortcut(key({ key: "F11" }), typing), null);
  assert.equal(matchShortcut(key({ key: "Escape" }), typing), null, "输入框里的 Esc 是「取消输入」");
  assert.equal(matchShortcut(key({ key: "s", ctrlKey: true }), typing), "save-now");
  assert.equal(matchShortcut(key({ key: "n", ctrlKey: true }), typing), "new-chapter");
});

test("Esc 只在「有东西可退」时才认（专注或全屏开着）", () => {
  assert.equal(matchShortcut(key({ key: "Escape" }), ctx()), null, "常规态按 Esc 不该有动作");
  assert.equal(matchShortcut(key({ key: "Escape" }), ctx({ zenOn: true })), "exit-focus");
  assert.equal(matchShortcut(key({ key: "Escape" }), ctx({ fullscreenOn: true })), "exit-focus");
  assert.equal(
    matchShortcut(key({ key: "Escape" }), ctx({ zenOn: true, fullscreenOn: true })),
    "exit-focus",
    "两个都开着也是「回到常规」一下收完",
  );
});

test("输入框判定只认 input / textarea——正文本体是 contenteditable，不算", () => {
  assert.equal(isTextFieldTarget({ tagName: "input" }), true);
  assert.equal(isTextFieldTarget({ tagName: "TEXTAREA" }), true);
  // 这一条是专注模式的命门：算成输入框的话，正文里按 Esc 就退不出专注
  assert.equal(isTextFieldTarget({ tagName: "DIV", isContentEditable: true }), false);
  assert.equal(isTextFieldTarget(null), false);
  assert.equal(isTextFieldTarget({}), false);
});

test("按钮上的键位提示是从表里生成的（键位改了提示跟着走）", () => {
  assert.equal(shortcutKeys("zen"), "F9");
  assert.equal(shortcutKeys("fullscreen"), "F11");
  assert.equal(shortcutKeys("save-now"), "Ctrl+S");
  assert.equal(shortcutKeys("prev-chapter"), "Ctrl+Alt+←");
  assert.equal(shortcutKeys("next-chapter"), "Ctrl+Alt+→");
});

test("键位表自身守规矩：不重复、可打印键必带修饰键、必带 id", () => {
  const seen = new Set<string>();
  for (const entry of SHORTCUTS) {
    const combo = `${entry.ctrl ? "C" : ""}${entry.alt ? "A" : ""}${entry.shift ? "S" : ""}${entry.key}`;
    assert.equal(seen.has(combo), false, `键位重复：${combo}`);
    seen.add(combo);
    if (entry.key.length === 1) {
      assert.ok(entry.ctrl || entry.alt, `单字符键必须带修饰键：${combo}`);
    }
  }
});
