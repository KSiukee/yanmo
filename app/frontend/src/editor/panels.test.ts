// 悬浮卡片的验收：**一次只开一张、再点收起、关掉是关掉**。

import { test } from "node:test";
import assert from "node:assert/strict";

import { useFloatingPanels } from "./panels.ts";

test("一开始都收着（专注进去时不该先弹一张卡盖住正文）", () => {
  const panels = useFloatingPanels();
  assert.equal(panels.open.value, null);
  assert.equal(panels.isOpen("outline"), false);
});

test("点一下唤出，再点一下收起（同一个按钮管两件事）", () => {
  const panels = useFloatingPanels();
  assert.equal(panels.toggle("outline"), true, "第一次点：开");
  assert.equal(panels.isOpen("outline"), true);
  assert.equal(panels.toggle("outline"), false, "第二次点：收");
  assert.equal(panels.isOpen("outline"), false);
});

test("close 一律收起（卡片上的「关闭」按钮走这条）", () => {
  const panels = useFloatingPanels();
  panels.toggle("outline");
  panels.close();
  assert.equal(panels.open.value, null);
});

test("一次只开一张：换一张开，前一张自动收（哪怕同一个 id 也不会叠）", () => {
  const panels = useFloatingPanels();
  panels.toggle("outline");
  panels.toggle("outline");
  panels.toggle("outline");
  assert.equal(panels.open.value, "outline", "反复点只该在开/收之间切，不会累积");
  panels.close();
  assert.equal(panels.open.value, null);
});
