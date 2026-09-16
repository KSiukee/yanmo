// 场景卡的四格：**单独一张小表单、单独存**。
//
// 与正文分开的理由和"章纲一句话"一样（见 `note.ts`）：它不参与正文的落盘防抖、
// 指纹校验与抢救链，也不该让那些多一个变量。它就是**低频的手填字段**：填完、存下。
//
// 三条分寸：
// - **不是场景卡就整块不出现**（`isScene` 为假时组件不渲染这张表单）；
// - **存下了才算数**（存不下去就别装作填好了）；
// - **切节点以核心给的为准**（编辑器里没存的草稿跟着丢掉——节点都换了）。
//
// 只认接口不认具体命令（真命令在会话层注入）：可以脱离界面与核心单测。

import { ref, type Ref } from "vue";

import type { SceneFields } from "../api/core.ts";

/** 保存四格（会话层注入真命令，测试注入替身）。 */
export interface SceneTransport {
  save: (fields: SceneFields) => Promise<SceneFields>;
}

export interface SceneOptions {
  transport: SceneTransport;
  /** 当前节点；没有节点就什么都做不了 */
  nodeId: Ref<number | null>;
  onError?: (message: string) => void;
}

export interface SceneState {
  /** 当前节点是不是场景卡（不是就整块不出现） */
  isScene: Ref<boolean>;
  /** 四格（**正在编辑的那一份**） */
  draft: Ref<SceneFields>;
  busy: Ref<boolean>;
  /** 存下之后的一句回执（换节点/再改动就清掉） */
  saved: Ref<boolean>;
  /** 存一遍（存的是手上这四格） */
  save: () => Promise<void>;
  /** 换节点：核心给什么就是什么（`null` = 不是场景卡，表单收起来） */
  reset: (fields: SceneFields | null) => void;
}

/** 空白的那一份（`node_id` 由 `reset` 时补）。 */
function blank(node_id: number): SceneFields {
  return { node_id, pov: "", goal: "", conflict: "", outcome: "" };
}

export function useSceneFields(options: SceneOptions): SceneState {
  const isScene = ref(false);
  const draft = ref<SceneFields>(blank(0));
  const busy = ref(false);
  const saved = ref(false);

  return {
    isScene,
    draft,
    busy,
    saved,
    save: async () => {
      const node_id = options.nodeId.value;
      if (node_id === null || !isScene.value || busy.value) return;
      busy.value = true;
      try {
        // 回执是**库里真有的那一份**（修剪过的），不是界面自己回显
        draft.value = await options.transport.save({ ...draft.value, node_id });
        saved.value = true;
      } catch (error) {
        options.onError?.(error instanceof Error ? error.message : String(error));
      } finally {
        busy.value = false;
      }
    },
    reset: (fields) => {
      saved.value = false;
      if (fields === null) {
        isScene.value = false;
        draft.value = blank(0);
        return;
      }
      isScene.value = true;
      draft.value = { ...fields };
    },
  };
}
