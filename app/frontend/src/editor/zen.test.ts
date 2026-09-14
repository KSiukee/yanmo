// 专注模式的验收：**露哪几块**与"进出后没留下痕迹"。
//
// 表驱动为主：这是纯判断，把组合一次钉住比只测一两个例子更能压住回归。

import { test } from "node:test";
import assert from "node:assert/strict";

import { ZEN_OFF, ZEN_ON, useZen, zenChrome } from "./zen.ts";

test("常规态：两侧栏与顶栏都露着", () => {
  assert.deepEqual(zenChrome(false), ZEN_OFF);
  assert.deepEqual(zenChrome(false), {
    directory: true,
    flow: true,
    topbar: true,
    editorBar: true,
  });
});

test("专注态：藏两侧栏与顶栏，**留**编辑器那条细状态栏", () => {
  const chrome = zenChrome(true);
  assert.deepEqual(chrome, ZEN_ON);
  assert.equal(chrome.directory, false, "目录栏要藏");
  assert.equal(chrome.flow, false, "叩问栏要藏");
  assert.equal(chrome.topbar, false, "顶栏要藏");
  // 这一条最容易在"顺手多藏一块"时被改坏：字数与今日进度是写字时的刚需
  assert.equal(chrome.editorBar, true, "字数/今日/保存状态那条必须留着");
});

test("进出专注：只改「露不露」，不改别的状态", () => {
  const zen = useZen();
  assert.equal(zen.on.value, false, "默认是常规三栏");
  assert.deepEqual(zen.chrome.value, ZEN_OFF);

  zen.enter();
  assert.equal(zen.on.value, true);
  assert.deepEqual(zen.chrome.value, ZEN_ON, "进了专注，chrome 跟着变");

  zen.exit();
  assert.equal(zen.on.value, false);
  assert.deepEqual(zen.chrome.value, ZEN_OFF, "退出后一模一样地回到常规");
});

test("toggle 返回切换后的状态（快捷键层要用它决定后续动作）", () => {
  const zen = useZen();
  assert.equal(zen.toggle(), true, "第一次切：进专注");
  assert.equal(zen.on.value, true);
  assert.equal(zen.toggle(), false, "再切：回常规");
  assert.equal(zen.on.value, false);
});
