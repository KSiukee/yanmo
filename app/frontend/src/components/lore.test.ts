// 「设定」弹窗外壳的接线验收：**打开才读、点哪条开哪页、收起时放下表单**。
//
// 两屏自己的规则在核心与各自的 panel 里；这里只盯"外壳做对了没有"。

import { test } from "node:test";
import assert from "node:assert/strict";
import { ref } from "vue";

import type { EntityPanelState } from "./entity-panel.ts";
import type { ForeshadowPanelState } from "./foreshadow-panel.ts";
import { useLore } from "./lore.ts";

/** 两屏的替身：只记"读了几次""表单收了几次"。 */
function fakePanes() {
  const loads = { entities: 0, foreshadows: 0 };
  const cancels = { entities: 0, foreshadows: 0 };
  const entities = {
    load: async () => {
      loads.entities += 1;
    },
    cancelEdit: () => {
      cancels.entities += 1;
    },
  } as unknown as EntityPanelState;
  const foreshadows = {
    load: async () => {
      loads.foreshadows += 1;
    },
    cancelEdit: () => {
      cancels.foreshadows += 1;
    },
  } as unknown as ForeshadowPanelState;
  return { loads, cancels, entities, foreshadows };
}

test("一打开就落到要的那一页，并读那一片（不是两片一起读）", async () => {
  const panes = fakePanes();
  const lore = useLore({ entities: panes.entities, foreshadows: panes.foreshadows });

  assert.equal(lore.visible.value, false);
  lore.show("foreshadows");
  await Promise.resolve();
  assert.equal(lore.visible.value, true);
  assert.equal(lore.tab.value, "foreshadows");
  assert.deepEqual(panes.loads, { entities: 0, foreshadows: 1 });
});

test("不给页签就停在上一页（顶栏那个入口是这么用的）", async () => {
  const panes = fakePanes();
  const lore = useLore({ entities: panes.entities, foreshadows: panes.foreshadows });

  lore.show("foreshadows");
  lore.hide();
  lore.show();
  await Promise.resolve();
  assert.equal(lore.tab.value, "foreshadows", "停在上一页");
  assert.equal(panes.loads.foreshadows, 2);
});

test("换页签：读新那一页", async () => {
  const panes = fakePanes();
  const lore = useLore({ entities: panes.entities, foreshadows: panes.foreshadows });
  lore.show("entities");
  await Promise.resolve();
  lore.pick("foreshadows");
  await Promise.resolve();
  assert.equal(lore.tab.value, "foreshadows");
  assert.deepEqual(panes.loads, { entities: 1, foreshadows: 1 });
});

test("收起：两屏手上的表单都放下（半填的不该下次冒出来）", () => {
  const panes = fakePanes();
  const lore = useLore({ entities: panes.entities, foreshadows: panes.foreshadows });
  lore.show();
  lore.hide();
  assert.equal(lore.visible.value, false);
  assert.deepEqual(panes.cancels, { entities: 1, foreshadows: 1 });
});

test("ref 由外壳拿着（组件据此渲染）", () => {
  const panes = fakePanes();
  const lore = useLore({ entities: panes.entities, foreshadows: panes.foreshadows });
  assert.equal(typeof lore.visible.value, "boolean");
  assert.ok(ref(lore.tab.value));
});
