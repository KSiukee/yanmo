// 删章路标：**点「+」前先问一嘴**——这一层少了一章，要补写吗？
//
// 三条分寸：
// - **有据才问**：只有"回收站里还躺着那一章"才算数；哪一层缺了第几号由核心算好摆出来，
//   界面不自己数空档（没有删除记录的空档留给交付前汇总，不在这儿打扰）；
// - **三态机在核心那一处**（没问过 → 稍后 → 再问一次 → 又稍后 → 自动降为「不用了」），
//   这里只转发答复，不另存一份真相；
// - **绝不替作者拿主意**：问出来就摆着，他选了什么就照什么走。
//
// 只认接口不认具体命令（真命令在会话层注入）：这套状态机可以脱离界面与核心单测。

import { ref, type Ref } from "vue";

import type { ChapterGap } from "../api/core";
import { formatWhen, formatWords } from "./display.ts";

/** 删章路标要用的几个动作（会话层注入真命令，测试注入替身）。 */
export interface GapTransport {
  /** 这一层现在该不该问一句 */
  check: (work_id: number, parent_id: number | null) => Promise<ChapterGap | null>;
  /** 记答复：稍后再说 / 不用了 */
  answer: (node_id: number, answer: "deferred" | "ignored") => Promise<void>;
  /** 补写：新建一个空章，返回新章 id */
  fill: (node_id: number) => Promise<number>;
}

export interface GapsOptions {
  transport: GapTransport;
  /** 当前作品（问的是这本书的某一层） */
  workId: Ref<number | null>;
  onError?: (message: string) => void;
}

export interface Gaps {
  /** 非空时界面要摆一次弹窗：补写 / 不用了 / 稍后再说 */
  pending: Ref<ChapterGap | null>;
  busy: Ref<boolean>;
  /** 点「+」前先问一嘴：这一层有该问的空缺就摆出来并返回 true（这次先别顺手建新章） */
  check: (parent_id: number | null) => Promise<boolean>;
  /** 作者选了「补写」：**是新建空章，不是恢复旧稿**；返回新章 id（没做成 null） */
  fill: () => Promise<number | null>;
  /** 作者选了「稍后再说 / 不用了」：记下答复，返回"可以接着建新章" */
  answer: (answer: "deferred" | "ignored") => Promise<boolean>;
  /** 关掉弹窗（去回收站看看 / 点遮罩）：**不记答复**，下次点「+」还问 */
  dismiss: () => void;
}

/** 弹窗上那几行字：哪儿缺了哪一章、什么时候删的、旧稿还有多少字。 */
export function gapNote(gap: ChapterGap): string {
  const where = gap.parent_title ? `「${gap.parent_title}」里缺了` : "目录里缺了";
  return `${where}${gap.title}——${formatWhen(gap.deleted_at)}删的，旧稿 ${formatWords(gap.word_count)} 字还在回收站里。`;
}

export function useGaps(options: GapsOptions): Gaps {
  const pending = ref<ChapterGap | null>(null);
  const busy = ref(false);
  const report = (error: unknown) => {
    options.onError?.(error instanceof Error ? error.message : String(error));
  };

  /** 问一嘴：核心说"没得问"就什么都不做；**问不出来也别拦着作者建章**。 */
  async function check(parent_id: number | null): Promise<boolean> {
    const work_id = options.workId.value;
    if (work_id === null) return false;
    try {
      const gap = await options.transport.check(work_id, parent_id);
      pending.value = gap;
      return gap !== null;
    } catch (error) {
      report(error);
      return false;
    }
  }

  /** 一次动作统一收口：忙标记 + 清掉待答的空缺 + 报错。 */
  async function act<T>(op: (gap: ChapterGap) => Promise<T>): Promise<T | null> {
    const gap = pending.value;
    if (!gap || busy.value) return null;
    busy.value = true;
    try {
      const result = await op(gap);
      pending.value = null;
      return result;
    } catch (error) {
      report(error);
      return null;
    } finally {
      busy.value = false;
    }
  }

  return {
    pending,
    busy,
    check,
    fill: () => act((gap) => options.transport.fill(gap.node_id)),
    answer: async (next) =>
      (await act(async (gap) => {
        await options.transport.answer(gap.node_id, next);
        return true;
      })) === true,
    dismiss: () => {
      pending.value = null;
    },
  };
}
