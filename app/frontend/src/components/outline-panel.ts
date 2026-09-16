// 大纲体检面板的**状态与命令编排**：扫一遍、忽略一处、捡回来、全部重看。
//
// 单独成文件的原因与 `creator-panel.ts` 同一条：这一整块是"会发生什么"，
// 而 `.vue` 那一份是"长什么样"。
//
// 三条分寸（与核心一致）：
// - **只读优先**：打开面板只是扫一遍，不动一个字节；
// - **忽略要记住**：记的是"这条问题的身份"（指纹），所以下一次打开它还在忽略里；
// - **回头路**：忽略过的能列出来、能捡回来、能全部重看（忽略不是销毁）。

import { computed, onMounted, ref, watch, type Ref } from "vue";

import { asError } from "../api/errors.ts";
import {
  outlineClearDismissed,
  outlineDismiss,
  outlineScan,
  outlineUndismiss,
  type OutlineBoard,
  type OutlineIssue,
} from "../api/outline.ts";
import { splitIssues } from "./outline.ts";

export interface OutlinePanelOptions {
  /**
   * 当前作品（换书＝换一张清单）。
   *
   * **收 Ref 而不是收值**：会话里那个 `workId` 本来就是 Ref，别的面板
   * （entity / foreshadow / creator / grid）也都收 Ref。这里原来写的是 `number | null`，
   * 于是调用处把 Ref 直接递了进来——`if (!work)` 判过、参数却是个对象，
   * 每次扫描都在 IPC 那一层炸掉（0.68.1 真机看见的就是它）。
   */
  workId: Ref<number | null>;
}

export function useOutlinePanel(props: OutlinePanelOptions) {
  const board = ref<OutlineBoard | null>(null);
  const busy = ref(false);
  /** 失败时那句**已经渲染好的**话（`CoreError` 走字典渲染） */
  const errorText = ref("");
  /** 已忽略的那一堆要不要摆出来 */
  const showKnown = ref(false);

  const split = computed(() => splitIssues(board.value?.issues ?? [], board.value?.dismissed ?? []));
  /** 还要看的（面板默认摆这一堆） */
  const open = computed(() => split.value.open);
  /** 已经知道的（作者点"看忽略了哪些"才摆） */
  const known = computed(() => split.value.known);

  function report(error: unknown) {
    errorText.value = asError(error).message;
  }

  /** 现在这本书的 id（没有书就什么都不做——面板在没书时是空的）。 */
  function currentWork(): number | null {
    return props.workId.value;
  }

  /** 扫一遍（**只读**）。 */
  async function refresh() {
    const work = currentWork();
    if (!work) {
      board.value = null;
      return;
    }
    try {
      busy.value = true;
      errorText.value = "";
      board.value = await outlineScan(work);
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  /** 一次处置的统一收尾：调命令、吃回最新那一屏。 */
  async function act(action: () => Promise<OutlineBoard>) {
    try {
      busy.value = true;
      errorText.value = "";
      board.value = await action();
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  const dismiss = (issue: OutlineIssue) => {
    const work = currentWork();
    return work ? act(() => outlineDismiss(work, issue.fingerprint)) : undefined;
  };
  const undismiss = (issue: OutlineIssue) => {
    const work = currentWork();
    return work ? act(() => outlineUndismiss(work, issue.fingerprint)) : undefined;
  };
  const clearDismissed = () => {
    const work = currentWork();
    return work ? act(() => outlineClearDismissed(work)) : undefined;
  };

  onMounted(() => void refresh());
  watch(
    () => props.workId.value,
    () => {
      showKnown.value = false;
      void refresh();
    },
  );

  return {
    board,
    busy,
    errorText,
    showKnown,
    open,
    known,
    refresh,
    dismiss,
    undismiss,
    clearDismissed,
  };
}
