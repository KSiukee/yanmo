// 「记住上次那一块」这一小片的接线验收：读回来之前用默认、读回来就跟着走、换一块写回去。
//
// 真正"存哪儿、怎么校验"在核心（`store::appearance` 有验收）；这里只盯界面这一层。

import { test } from "node:test";
import assert from "node:assert/strict";
import { ref } from "vue";

import type { AppearanceState } from "./appearance.ts";
import { DEFAULT_ASIDE_TAB, useAsideTab } from "./aside.ts";

/** 偏好那一层的替身：只管 `workValues.aside_tab` 与写回来的那一笔。 */
function fakeAppearance(initial: string | null = null) {
  const workValues = ref<{ aside_tab: string } | null>(
    initial === null ? null : { aside_tab: initial },
  );
  const written: string[] = [];
  const appearance = {
    workValues,
    setAsideTab: async (value: string) => {
      written.push(value);
      workValues.value = { aside_tab: value };
    },
  } as unknown as AppearanceState;
  return { appearance, workValues, written };
}

test("库里那份还没读回来时先露默认那一块（不猜上次）", () => {
  const { appearance } = fakeAppearance(null);
  const aside = useAsideTab(appearance);
  assert.equal(aside.tab.value, DEFAULT_ASIDE_TAB);
  assert.equal(DEFAULT_ASIDE_TAB, "flow");
});

test("读回来之后跟着上次那一块走", () => {
  const { appearance, workValues } = fakeAppearance("creator");
  const aside = useAsideTab(appearance);
  assert.equal(aside.tab.value, "creator", "打开就该是上次那一块");

  // 之后偏好再变（比如设置面板里改的、或换书触发的重读）也跟着走
  workValues.value = { aside_tab: "flow" };
  assert.equal(aside.tab.value, "flow");
});

test("认不出的值当没设过：界面停在原来那一块，不被带歪", () => {
  const { appearance, workValues } = fakeAppearance("creator");
  const aside = useAsideTab(appearance);
  workValues.value = { aside_tab: "timeline" };
  assert.equal(aside.tab.value, "creator");
});

test("换一块：界面立刻跟着动，并写回偏好（写的是人家给的取值）", () => {
  const { appearance, written } = fakeAppearance(null);
  const aside = useAsideTab(appearance);

  aside.pick("creator");
  assert.equal(aside.tab.value, "creator", "先动界面，不等写库");
  assert.deepEqual(written, ["creator"], "再把选择写回去");
});
