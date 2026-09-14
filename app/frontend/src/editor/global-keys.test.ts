// 快捷键**接线**的验收：按键进来之后，到底有没有接到动作上。
//
// 用一个假窗口，把"命中 / 不命中 / 不抢键"整条链路跑一遍——
// 真机上那些毛病（组字时换章、输入框里被功能键打断）都在这里挡下来。

import { test } from "node:test";
import assert from "node:assert/strict";

import { attachGlobalKeys, runShortcut, type GlobalKeyDeps } from "./global-keys.ts";
import { SHORTCUTS, type ShortcutId } from "./shortcuts.ts";

/** 动作记录器 + 可改的界面状态（模拟"弹窗开着""专注开着"这些情形）。 */
function makeDeps() {
  const calls: string[] = [];
  const state = { dialogOpen: false, zenOn: false, fullscreenOn: false };
  const deps: GlobalKeyDeps = {
    dialogOpen: () => state.dialogOpen,
    zenOn: () => state.zenOn,
    fullscreenOn: () => state.fullscreenOn,
    toggleZen: () => calls.push("toggleZen"),
    toggleFullscreen: () => calls.push("toggleFullscreen"),
    exitFocus: () => calls.push("exitFocus"),
    prevChapter: () => calls.push("prevChapter"),
    nextChapter: () => calls.push("nextChapter"),
    saveNow: () => calls.push("saveNow"),
    newChapter: () => calls.push("newChapter"),
    openSettings: () => calls.push("openSettings"),
    openShelf: () => calls.push("openShelf"),
  };
  return { deps, calls, state };
}

/** 假窗口：只实现"挂监听 / 摘监听"，再把一个按键事件递给监听者。 */
class FakeWindow {
  private listeners: ((event: Event) => void)[] = [];

  get attached(): number {
    return this.listeners.length;
  }

  addEventListener(_type: string, listener: (event: Event) => void): void {
    this.listeners.push(listener);
  }

  removeEventListener(_type: string, listener: (event: Event) => void): void {
    this.listeners = this.listeners.filter((item) => item !== listener);
  }

  /** 按一下；返回"有没有被拦下来"（preventDefault / stopPropagation）。 */
  press(init: {
    key: string;
    ctrlKey?: boolean;
    altKey?: boolean;
    shiftKey?: boolean;
    isComposing?: boolean;
    repeat?: boolean;
    target?: unknown;
  }): { prevented: boolean; stopped: boolean } {
    let prevented = false;
    let stopped = false;
    const event = {
      key: init.key,
      ctrlKey: Boolean(init.ctrlKey),
      altKey: Boolean(init.altKey),
      shiftKey: Boolean(init.shiftKey),
      metaKey: false,
      isComposing: init.isComposing,
      repeat: init.repeat,
      target: init.target ?? null,
      preventDefault: () => {
        prevented = true;
      },
      stopPropagation: () => {
        stopped = true;
      },
    } as unknown as Event;
    for (const listener of this.listeners) listener(event);
    return { prevented, stopped };
  }
}

test("认得的组合键：接到动作上，并且**拦下默认行为**", () => {
  const { deps, calls } = makeDeps();
  const win = new FakeWindow();
  const detach = attachGlobalKeys(deps, win);

  // 拦截是必须的：WebView2 自己也认 Ctrl+N / Ctrl+S / F11，不拦会同时开两套
  const saved = win.press({ key: "s", ctrlKey: true });
  assert.deepEqual(calls, ["saveNow"]);
  assert.deepEqual(saved, { prevented: true, stopped: true }, "命中的键必须拦下来");

  win.press({ key: "n", ctrlKey: true });
  win.press({ key: "F9" });
  win.press({ key: "F11" });
  win.press({ key: "b", ctrlKey: true });
  win.press({ key: ",", ctrlKey: true });
  win.press({ key: "ArrowRight", ctrlKey: true, altKey: true });
  win.press({ key: "ArrowLeft", ctrlKey: true, altKey: true });
  assert.deepEqual(calls, [
    "saveNow",
    "newChapter",
    "toggleZen",
    "toggleFullscreen",
    "openShelf",
    "openSettings",
    "nextChapter",
    "prevChapter",
  ]);

  detach();
});

test("不认的键：**不拦也不动**（正文照常收到这个键）", () => {
  const { deps, calls } = makeDeps();
  const win = new FakeWindow();
  attachGlobalKeys(deps, win);

  const plain = win.press({ key: "s" });
  assert.deepEqual(calls, [], "单按字母不该有动作");
  assert.deepEqual(plain, { prevented: false, stopped: false }, "不认的键绝不能拦");

  const half = win.press({ key: "ArrowRight", ctrlKey: true });
  assert.deepEqual(calls, [], "少按一个修饰键就不算换章");
  assert.equal(half.prevented, false);

  const none = win.press({ key: "Escape" });
  assert.deepEqual(calls, [], "常规态按 Esc 没有动作，交给编辑器自己");
  assert.equal(none.prevented, false);
});

test("输入法组字中：不拦也不动", () => {
  const { deps, calls } = makeDeps();
  const win = new FakeWindow();
  attachGlobalKeys(deps, win);

  const composing = win.press({ key: "s", ctrlKey: true, isComposing: true });
  assert.deepEqual(calls, []);
  assert.equal(composing.prevented, false, "组字中拦住 Ctrl+S 会影响输入法");
});

test("长按连发：只算第一次（repeat 不认）", () => {
  const { deps, calls } = makeDeps();
  const win = new FakeWindow();
  attachGlobalKeys(deps, win);

  win.press({ key: "F9" });
  win.press({ key: "F9", repeat: true });
  win.press({ key: "F9", repeat: true });
  assert.deepEqual(calls, ["toggleZen"], "按住不放只该切一次");
});

test("弹窗开着：一律不认（Esc 归弹窗）", () => {
  const { deps, calls, state } = makeDeps();
  const win = new FakeWindow();
  attachGlobalKeys(deps, win);
  state.dialogOpen = true;
  state.zenOn = true;

  assert.equal(win.press({ key: "F9" }).prevented, false);
  assert.equal(win.press({ key: "Escape" }).prevented, false);
  win.press({ key: "n", ctrlKey: true });
  assert.deepEqual(calls, [], "弹窗开着时界面快捷键全部让位");
});

test("焦点在输入框里：功能键让位，带修饰键的组合照常", () => {
  const { deps, calls } = makeDeps();
  const win = new FakeWindow();
  attachGlobalKeys(deps, win);
  const input = { tagName: "INPUT" };

  assert.equal(win.press({ key: "F9", target: input }).prevented, false);
  assert.deepEqual(calls, []);
  win.press({ key: "s", ctrlKey: true, target: input });
  assert.deepEqual(calls, ["saveNow"], "在输入框里也想「立刻落盘」");
});

test("正文里按 Esc：退出专注（正文本体是 contenteditable，不算输入框）", () => {
  const { deps, calls, state } = makeDeps();
  const win = new FakeWindow();
  attachGlobalKeys(deps, win);
  state.zenOn = true;

  const body = { tagName: "DIV", isContentEditable: true };
  const pressed = win.press({ key: "Escape", target: body });
  assert.deepEqual(calls, ["exitFocus"]);
  assert.equal(pressed.prevented, true, "退专注这一下要拦下，别让编辑器再处理一遍");
});

test("解绑之后再按：一点反应都没有（监听也摘干净了）", () => {
  const { deps, calls } = makeDeps();
  const win = new FakeWindow();
  const detach = attachGlobalKeys(deps, win);
  assert.equal(win.attached, 1, "挂上一个监听");

  detach();
  assert.equal(win.attached, 0, "摘干净");
  win.press({ key: "F9" });
  assert.deepEqual(calls, []);
});

test("每个键位都接上了动作（表里加了键却没接线，这条会红）", () => {
  const expected: Record<ShortcutId, string> = {
    zen: "toggleZen",
    fullscreen: "toggleFullscreen",
    "prev-chapter": "prevChapter",
    "next-chapter": "nextChapter",
    "save-now": "saveNow",
    "new-chapter": "newChapter",
    settings: "openSettings",
    shelf: "openShelf",
    "exit-focus": "exitFocus",
  };
  // Esc 是特判（不在表里），其余每一个表项都要有自己的动作
  const ids: ShortcutId[] = [...SHORTCUTS.map((entry) => entry.id), "exit-focus"];
  assert.equal(new Set(ids).size, ids.length, "同一件事不该在表里出现两次");
  for (const id of ids) {
    const { deps, calls } = makeDeps();
    runShortcut(id, deps);
    assert.deepEqual(calls, [expected[id]], `键位 ${id} 没接到「${expected[id]}」上`);
  }
});
