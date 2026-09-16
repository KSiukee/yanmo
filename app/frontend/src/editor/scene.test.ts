// 场景卡四格那一片的接线验收：**不是场景卡就不出现、存下了才算数、切节点以核心为准**。
//
// 真正的读写规则在核心（`store::scene_card` 有验收）；这里只盯界面这一层。

import { test } from "node:test";
import assert from "node:assert/strict";
import { ref } from "vue";

import type { SceneFields } from "../api/core.ts";
import { useSceneFields } from "./scene.ts";

function fakeCore(receipt?: (fields: SceneFields) => SceneFields) {
  const calls: SceneFields[] = [];
  return {
    calls,
    transport: {
      save: async (fields: SceneFields) => {
        calls.push(fields);
        return receipt ? receipt(fields) : fields;
      },
    },
  };
}

function fields(node_id: number, pov = "林昭"): SceneFields {
  return { node_id, pov, goal: "", conflict: "", outcome: "" };
}

test("不是场景卡：表单整块不出现，存也无从谈起", async () => {
  const core = fakeCore();
  const state = useSceneFields({ transport: core.transport, nodeId: ref<number | null>(7) });
  state.reset(null);
  assert.equal(state.isScene.value, false);

  await state.save();
  assert.equal(core.calls.length, 0, "不是场景卡不该往核心发写请求");
});

test("换到场景卡：核心给什么就是什么", () => {
  const core = fakeCore();
  const state = useSceneFields({ transport: core.transport, nodeId: ref<number | null>(7) });
  state.reset(fields(7, "陆文"));
  assert.equal(state.isScene.value, true);
  assert.equal(state.draft.value.pov, "陆文");
  assert.equal(state.draft.value.node_id, 7);
});

test("存一遍：带上当前节点，回执是核心给的那一份", async () => {
  const core = fakeCore((sent) => ({ ...sent, pov: sent.pov.trim(), goal: "" }));
  const state = useSceneFields({ transport: core.transport, nodeId: ref<number | null>(7) });
  state.reset(fields(7, "  林昭  "));

  await state.save();
  assert.deepEqual(core.calls[0], fields(7, "  林昭  "), "界面原样交上去（修剪是核心的事）");
  assert.equal(core.calls[0].node_id, 7);
  assert.equal(state.draft.value.pov.trim(), "林昭", "显示的是库里那份");
  assert.equal(state.saved.value, true);
});

test("切节点：回执清掉；存失败只报一句、不算存下", async () => {
  const state = useSceneFields({
    transport: { save: async () => Promise.reject(new Error("库写不进去")) },
    nodeId: ref<number | null>(7),
    onError: (message) => messages.push(message),
  });
  const messages: string[] = [];
  state.reset(fields(7));
  await state.save();
  assert.deepEqual(messages, ["库写不进去"]);
  assert.equal(state.saved.value, false, "存不下去就别装作填好了");

  state.reset(fields(9));
  assert.equal(state.saved.value, false);
  assert.equal(state.draft.value.node_id, 9);
});
