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
  outlineMentions,
  outlineScan,
  outlineUndismiss,
  type OutlineBoard,
  type OutlineIssue,
  type UnlistedBoard,
} from "../api/outline.ts";
import { entityCreate } from "../api/entity.ts";
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

  // ── 正文里冒出来的名字（这一块要读每一章正文，所以自己带一个"扫一遍"） ──
  const mentions = ref<UnlistedBoard | null>(null);
  const mentionsBusy = ref(false);
  /** 还要看的候选（忽略过的不摆在这一堆里） */
  const mentionOpen = computed(
    () => splitIssues(mentions.value?.issues ?? [], mentions.value?.dismissed ?? []).open,
  );

  /** 扫一遍正文（**只读**，也是这一块唯一的贵动作）。 */
  async function scanMentions() {
    const work = currentWork();
    if (!work) {
      mentions.value = null;
      return;
    }
    try {
      mentionsBusy.value = true;
      errorText.value = "";
      mentions.value = await outlineMentions(work);
    } catch (error) {
      report(error);
    } finally {
      mentionsBusy.value = false;
    }
  }

  /**
   * 「这不是名字」：与体检共用同一把忽略标记，写完**只重扫这一块**。
   *
   * 为什么不复用 `act()`：那一个是"调命令、吃回体检那一屏"，而这一条命令回的是
   * 另一份（[`UnlistedBoard`]）——吃错了会把候选混进体检清单里。
   */
  async function dismissMention(issue: OutlineIssue) {
    const work = currentWork();
    if (!work) return;
    try {
      mentionsBusy.value = true;
      errorText.value = "";
      await outlineDismiss(work, issue.fingerprint);
      mentions.value = await outlineMentions(work);
    } catch (error) {
      report(error);
    } finally {
      mentionsBusy.value = false;
    }
  }

  /**
   * 「建成人物卡 / 设定卡」：走既有的 `entity_create`（名字预填，其余空着），
   * 建完重扫——卡在那儿了，这一条自然就不再是"认不出来"。
   *
   * 这是这一块**唯一的写**（除了忽略），留痕由核心那条命令负责。
   */
  async function createCardFromMention(issue: OutlineIssue) {
    const work = currentWork();
    const name = (issue.params.form ?? "").trim();
    if (!work || name === "") return;
    try {
      mentionsBusy.value = true;
      busy.value = true;
      errorText.value = "";
      await entityCreate(work, {
        kind: issue.params.kind === "setting" ? "setting" : "person",
        name,
        aliases: [],
        attributes: [],
        note: "",
      });
      // 体检那一屏也重扫一遍：新卡可能带出别的发现（比如跟已有的别称撞了车）
      await refresh();
      mentions.value = await outlineMentions(work);
    } catch (error) {
      report(error);
    } finally {
      mentionsBusy.value = false;
      busy.value = false;
    }
  }

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
      // 换书＝换一把候选：上一本认出来的名字不许留在这一屏（下一本要重扫才会出来）
      mentions.value = null;
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
    mentions,
    mentionsBusy,
    mentionOpen,
    scanMentions,
    dismissMention,
    createCardFromMention,
  };
}
