// 「资料」弹窗外壳的接线验收：**打开才读、点哪页读哪页、切页/收起时放下表单**。
//
// 四页各自的规则在核心与各自的 panel 里；这里只盯"外壳做对了没有"。
// （总纲那一页 0.68.1 挪去了「大纲」第一页，不再归这里管。）

import { test } from "node:test";
import assert from "node:assert/strict";

import type { EntityPanelState } from "./entity-panel.ts";
import type { ForeshadowPanelState } from "./foreshadow-panel.ts";
import { useLore, type LoreOptions, type LoreTab } from "./lore.ts";

/** 各页的替身：只记"读了几次""表单收了几次"。 */
function fakePanes() {
  const loads: Record<string, number> = { entities: 0, foreshadows: 0, events: 0 };
  let cancels = 0;
  const entities = {
    load: async () => {
      loads.entities += 1;
    },
  } as unknown as EntityPanelState;
  const foreshadows = {
    load: async () => {
      loads.foreshadows += 1;
    },
  } as unknown as ForeshadowPanelState;
  const options: LoreOptions = {
    entities,
    foreshadows,
    refreshEvents: async () => {
      loads.events += 1;
    },
    cancelForms: () => {
      cancels += 1;
    },
  };
  return { loads, options, cancels: () => cancels };
}

async function opened(tab?: LoreTab) {
  const panes = fakePanes();
  const lore = useLore(panes.options);
  lore.show(tab);
  await Promise.resolve();
  return { lore, panes };
}

test("默认停在「人物」那一页（先有人）", () => {
  const panes = fakePanes();
  const lore = useLore(panes.options);
  assert.equal(lore.visible.value, false);
  assert.equal(lore.tab.value, "persons");
});

test("打开就落到要的那一页，只读那一页", async () => {
  const { lore, panes } = await opened("events");
  assert.equal(lore.visible.value, true);
  assert.equal(lore.tab.value, "events");
  assert.deepEqual(panes.loads, { entities: 0, foreshadows: 0, events: 1 });
});

test("人物与设定两页共用同一份名单（读一次就够）", async () => {
  const { lore, panes } = await opened("settings");
  assert.equal(panes.loads.entities, 1);
  lore.pick("persons");
  await Promise.resolve();
  assert.equal(panes.loads.entities, 2, "换页签就重读一次（别拿旧账给作者看）");
  assert.equal(panes.loads.events + panes.loads.foreshadows, 0);
});

test("事件那一页读的是碎片池那一份数据（不另开一条路）", async () => {
  const { panes } = await opened("events");
  assert.equal(panes.loads.events, 1);
  assert.equal(panes.loads.entities + panes.loads.foreshadows, 0);
});

test("伏笔那一页只读伏笔", async () => {
  const { panes } = await opened("foreshadows");
  assert.equal(panes.loads.foreshadows, 1);
  assert.equal(panes.loads.entities + panes.loads.events, 0);
});

test("换页签与收起：都把手上的表单放下（半填的不该下次冒出来）", async () => {
  const { lore, panes } = await opened("persons");
  assert.equal(panes.cancels(), 0);
  lore.pick("settings");
  assert.equal(panes.cancels(), 1);
  lore.hide();
  assert.equal(panes.cancels(), 2);
  assert.equal(lore.visible.value, false);
});


test("不给页签就停在上一页（顶栏那个入口是这么用的）", async () => {
  const { lore, panes } = await opened("foreshadows");
  lore.hide();
  lore.show();
  await Promise.resolve();
  assert.equal(lore.tab.value, "foreshadows");
  assert.equal(panes.loads.foreshadows, 2);
});
