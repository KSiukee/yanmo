// 「大纲 → 场景卡」那一页的**状态与命令编排**：列全书场景卡、就地填四格、跳到那一场。
//
// 与 `editor/scene.ts` 的分工：那一份是**编辑器里开着某一张卡时**顺手填的小表单；
// 这一份是**一眼看全书**的那一页（哪几张还没填、缺什么）。两份共用同一套核心命令。
//
// 三条分寸：
// 1. **就地填**：改哪一张只发哪一张（不整屏重存）；
// 2. **缺项看得见**：列表上直接标"还缺 2 格"（数据来自核心的 `missing`，不在这儿判）；
// 3. 它不是常驻画面：读的时机由外壳（打开面板 / 换页签）说了算。

import { ref, watch, type Ref } from "vue";

import { asError } from "../api/errors.ts";
import { sceneFieldsSave, sceneList, type SceneCard } from "../api/scene.ts";
import type { SceneFields } from "../api/core.ts";

export interface ScenePanelOptions {
  workId: Ref<number | null>;
}

export interface ScenePanelState {
  cards: Ref<SceneCard[]>;
  busy: Ref<boolean>;
  /** 失败时那句已经渲染好的话（`CoreError` 走字典渲染） */
  errorText: Ref<string>;
  /** 刚存下的那一张（界面上给一句回执） */
  justSaved: Ref<number | null>;
  load: () => Promise<void>;
  /** 存某一张的四格（只发这一张） */
  save: (fields: SceneFields) => Promise<void>;
}

export function useScenePanel(deps: ScenePanelOptions): ScenePanelState {
  const cards = ref<SceneCard[]>([]);
  const busy = ref(false);
  const errorText = ref("");
  const justSaved = ref<number | null>(null);

  function report(error: unknown) {
    errorText.value = asError(error).message;
  }

  async function load() {
    const work = deps.workId.value;
    if (!work) {
      cards.value = [];
      return;
    }
    try {
      busy.value = true;
      errorText.value = "";
      cards.value = await sceneList(work);
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  async function save(fields: SceneFields) {
    try {
      busy.value = true;
      errorText.value = "";
      const saved = await sceneFieldsSave(fields);
      // 回执是**库里真有的那一份**（修剪过的）：拿它换掉手上那一张
      cards.value = cards.value.map((card) =>
        card.node_id === saved.node_id ? { ...card, fields: saved } : card,
      );
      justSaved.value = saved.node_id;
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  watch(
    () => deps.workId.value,
    () => {
      // 换书：这一屏整个换一份（页签切回来时会重读）
      cards.value = [];
      justSaved.value = null;
      errorText.value = "";
    },
  );

  return { cards, busy, errorText, justSaved, load, save };
}
