// 「答一条之后要做的所有事」：把答案存进去 → 按走法决定攒着还是就地落 → 回执 → 接着问下一张。
//
// 单独成件的原因：这一串是**一个动作的完整链路**（它自己就够长），而面板那一层管的是
// "挑什么来问、面板长什么样"。拆开之后两边都能单独看懂；走法（先问后排版 / 跟章走）
// 从外面传进来，这里不自己判。
import { t } from "../locales/index.ts";
import { questionAnswer, type Answer, type QuestionBoard, type SelectedQuestion } from "../api/question.ts";
import type { EditorSession } from "../editor/session.ts";
import type { Ref, ComputedRef } from "vue";

import { defaultTarget, inputLabel } from "./question.ts";
import type { useLanding } from "./use-landing.ts";
import type { useRound } from "./use-round.ts";

export interface AnswerFlowOptions {
  session: EditorSession;
  /** 正在问的那一张（没有就什么都不做） */
  active: Ref<SelectedQuestion | null>;
  busy: Ref<boolean>;
  errorCode: Ref<string>;
  justSaved: Ref<string>;
  answerFor: Ref<boolean>;
  /** 现在走的是「先问后排版」：答下的先攒着，不就地落 */
  roundMode: ComputedRef<boolean>;
  /** 跟着章走（模式 A / B）：答完自动出下一张 */
  followChapter: ComputedRef<boolean>;
  currentChapter: ComputedRef<number | null>;
  /** 两半落法（就地落 / 一轮落），见各自的文件 */
  landing: ReturnType<typeof useLanding>;
  round: ReturnType<typeof useRound>;
  /** 把新面板交给面板那一层（它管着 board） */
  setBoard: (board: QuestionBoard) => void;
  /** 按面板那套口径再读一次（跟章走时带上「这一章优先」） */
  settle: (board: QuestionBoard) => Promise<QuestionBoard>;
  askNext: () => Promise<void>;
  report: (error: unknown) => void;
}

export function useAnswerFlow(options: AnswerFlowOptions) {
  /**
   * 作答：答案进答案池，问题卡就此走到终态。
   *
   * `source` 现在只有 `typed`（键盘）；口述那条链路落地后，那条路把 `voice` / `mixed`
   * 传进来就行——命令与存储的形状都不变，这正是"文本与输入方式解耦"要的效果。
   *
   * 落不落、怎么落，看当前走法：
   * - **先问后排版**：攒进这一轮（先不落），一轮问够了再一起落；
   * - 其余：勾了「落进正文」就**就地落**（答复制里还能补落）。
   */
  async function saveAnswer(body: string) {
    const card = options.active.value;
    if (!card || !body.trim()) return;
    try {
      options.busy.value = true;
      options.errorCode.value = "";
      // 这一版只有键盘这一条通道；口述那条链路落地后，这里换成 voice / mixed 就行
      const source = "typed";
      const receipt = await questionAnswer(card.card_id, body, source);
      const node = options.currentChapter.value;
      // 回执照**核心落下的那一份**说（不是照界面自己传的那份）：修剪过的原文、认下的输入方式
      let done = t("flow.answer.saved", {
        kind: inputLabel(receipt.answer.source),
        body: receipt.answer.body,
      });
      if (options.roundMode.value) {
        // 默认落点按要素类型分（plan 那一类答的就是章纲，其余落正文）；托盘里逐条还能改
        options.round.collect(receipt.answer as Answer, defaultTarget(card.element));
      } else {
        options.landing.noteAnswer(receipt.answer);
        const missed =
          options.landing.landOnAnswer.value &&
          !(await options.landing.landOne(card.card_id, receipt.answer.body, node));
        if (missed) done = t("flow.answer.land_failed", { body: receipt.answer.body });
      }
      options.setBoard(await options.settle(receipt.board));
      options.active.value = null;
      options.answerFor.value = false;
      // 跟章走：答完接着问下一张。回执要写在它**后面**——ask 会把上一张的回执清掉
      if (options.followChapter.value) await options.askNext();
      options.justSaved.value = done;
    } catch (error) {
      options.report(error);
    } finally {
      options.busy.value = false;
    }
  }

  return { saveAnswer };
}
