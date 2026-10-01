// 编辑器里那个「拆章」按钮要的那点状态：**能不能点、点开问一句、拆完收起来**。
//
// 为什么单独一件：`EditorPane.vue` 只该管"长什么样"（它已经贴着行数上限），
// 而"光标一动重算一次、点开算便宜的那一遍"是**行为**，与版式不是同一个变化理由。
//
// 两处口径来自作者拍板与核心：
// - **切点按段落算**（`editor/split.ts`，纯逻辑）；
// - **能不能点跟着光标走**：tiptap 的 state 不是 Vue 响应式的，所以用一个 tick 计数器
//   把 `selectionUpdate` 接进来——只挂监听，不改编辑器的任何行为。

import { computed, onMounted, ref, type Ref, type ShallowRef } from "vue";
import type { Editor } from "@tiptap/vue-3";

import { t } from "../locales/index.ts";
import { canSplit, splitPointOf, type SplitPoint } from "../editor/split.ts";

/** 只用到编辑器这两样（单测能喂一个假的进去，不必真起一个编辑器）。 */
export interface SplittableEditor {
  state: {
    doc: Parameters<typeof splitPointOf>[0];
    selection: { from: number };
  };
  on: (event: string, handler: () => void) => void;
}

export interface SplitAction {
  /** 现在这一处能不能拆（按钮据此置灰；光标一动就重算） */
  canSplitHere: Ref<boolean>;
  /** 打开确认层时的那一份切点（`null` = 没开着） */
  dialog: Ref<SplitPoint | null>;
  /** 点过但切不了时那一句解释（`""` = 没有） */
  hint: Ref<string>;
  /** 确认层正忙（拆章那一下还没回来） */
  busy: Ref<boolean>;
  /** 点「拆章」：再算一遍再问 */
  open: () => void;
  /** 确认层关掉 */
  close: () => void;
  /** 确认：真的动手（由会话编排落盘与切章） */
  confirm: (title: string) => Promise<void>;
}

/**
 * `editor` 收 `ShallowRef`（会话里那个本来就是），`run` 是"真去拆"那一件事
 * （会话的 `splitChapterAt`）——这里不认识命令、不认识 Tauri。
 */
export function useSplitAction(
  editor: ShallowRef<Editor | undefined>,
  run: (at: number, title: string) => Promise<boolean>,
): SplitAction {
  const tick = ref(0);
  const dialog = ref<SplitPoint | null>(null);
  const hint = ref("");
  const busy = ref(false);

  onMounted(() => {
    editor.value?.on("selectionUpdate", () => {
      tick.value += 1;
    });
  });

  const canSplitHere = computed(() => {
    void tick.value; // 光标一动就重算（依赖这一下）
    return canSplit((editor.value as SplittableEditor | undefined) ?? null);
  });

  function open() {
    hint.value = "";
    const instance = editor.value as SplittableEditor | undefined;
    const point = instance
      ? splitPointOf(instance.state.doc, instance.state.selection.from)
      : null;
    if (point === null) {
      hint.value = t("split.cannot");
      return;
    }
    dialog.value = point;
  }

  function close() {
    dialog.value = null;
  }

  async function confirm(title: string) {
    const point = dialog.value;
    if (point === null) return;
    busy.value = true;
    try {
      await run(point.at, title);
    } finally {
      busy.value = false;
      dialog.value = null;
    }
  }

  return { canSplitHere, dialog, hint, busy, open, close, confirm };
}
