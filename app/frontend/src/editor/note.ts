// 当前章的"一句话"：**单独一行、单独存**。
//
// 为什么不跟正文混在一起：正文有自己的落盘防抖、指纹校验与抢救链，"一句话"不参与那些，
// 也不该让它们多一个变量。它就是一个**低频的手填字段**：点开、写完、存下。
//
// 两条分寸：
// - **存下了才改显示**（存不下去就别装作写好了）；
// - 切章时**以核心给的为准**（编辑器里没存的草稿跟着丢掉——章都换了）。
//
// 只认接口不认具体命令（真命令在会话层注入）：这套状态机可以脱离界面与核心单测。

import { ref, type Ref } from "vue";

/** 保存这一章的一句话（会话层注入真命令，测试注入替身）。 */
export interface NoteTransport {
  save: (node_id: number, text: string) => Promise<void>;
}

export interface NoteOptions {
  transport: NoteTransport;
  /** 当前这一章；没有章就什么都做不了 */
  nodeId: Ref<number | null>;
  onError?: (message: string) => void;
}

export interface ChapterNote {
  /** 已经存下的那一句（空串＝没写过） */
  value: Ref<string>;
  /** 正在编辑的那一份 */
  draft: Ref<string>;
  editing: Ref<boolean>;
  busy: Ref<boolean>;
  /** 打开输入框：拿存下那份起头 */
  open: () => void;
  cancel: () => void;
  save: () => Promise<void>;
  /** 打开/切到某一章：核心给什么就是什么（没给就按"没写过"处理） */
  reset: (text?: string) => void;
}

export function useChapterNote(options: NoteOptions): ChapterNote {
  const value = ref("");
  const draft = ref("");
  const editing = ref(false);
  const busy = ref(false);

  return {
    value,
    draft,
    editing,
    busy,
    open: () => {
      draft.value = value.value;
      editing.value = true;
    },
    cancel: () => {
      editing.value = false;
    },
    save: async () => {
      const node_id = options.nodeId.value;
      if (node_id === null || busy.value) return;
      busy.value = true;
      try {
        await options.transport.save(node_id, draft.value);
        value.value = draft.value; // 存下了才改显示
        editing.value = false;
      } catch (error) {
        options.onError?.(error instanceof Error ? error.message : String(error));
      } finally {
        busy.value = false;
      }
    },
    reset: (text) => {
      // 核心没给（老壳/坏记录）：按"没写过"处理，别把 undefined 摆到界面上
      const safe = text ?? "";
      value.value = safe;
      draft.value = safe;
      editing.value = false;
    },
  };
}
