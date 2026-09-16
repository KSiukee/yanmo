// 「先问后排版」的一轮：**攒着 → 整理 → 一次落**。
//
// 与「答一条落一条」（`use-landing.ts`）是两半：这一半管"先不落，攒着"，那一半管
// "就地落"。两条路的落法共用同一件事——**正文那几段字由界面插进编辑会话**，
// 核心只做账（见 `api/question.ts` 里两个命令的注释）。
//
// 单独成文件的原因同别的组合式：这套状态（托盘、预览、落）与"面板长什么样"、
// "挑哪些问题来问"都不是一回事。
import { computed, ref } from "vue";

import { asError } from "../api/errors.ts";
import {
  questionApplyRound,
  type Answer,
  type AnswerTarget,
  type LandReceipt,
  type QuestionBoard,
  type RoundItem,
} from "../api/question.ts";
import type { EditorSession } from "../editor/session.ts";
import type { LandAt } from "./question.ts";
import { amendAt, appendRound, moveAt, removeAt, roundHasBody, roundHasText, roundText } from "./round.ts";

export interface RoundOptions {
  session: EditorSession;
  workId: () => number | null;
  /** 当前这一章（落点） */
  currentNode: () => number | null;
  /** 落完之后的新面板，交回面板那一层 */
  onBoard: (board: QuestionBoard) => void;
  /** 核心记下的账（章纲成了什么 / 新建了哪些场景卡）：界面按它更新"一句话"与目录树 */
  onLanded?: (landed: LandReceipt) => void;
  /** 模式 A 的口径：要不要按「这一章优先」再读一次（由面板那一层定） */
  settle: (board: QuestionBoard) => Promise<QuestionBoard>;
}

export function useRound(options: RoundOptions) {
  /** 这一轮攒下的（顺序就是落下去的顺序） */
  const items = ref<RoundItem[]>([]);
  /** 作者点了「一轮问够了」：进"先看一眼再落"那一步 */
  const previewing = ref(false);

  const count = computed(() => items.value.length);
  const hasText = computed(() => roundHasText(items.value));

  /** 答下一条：攒进这一轮（先不落）；`target` 是面板按要素类型给的默认落点，托盘里还能改 */
  function collect(answer: Answer, target: AnswerTarget = "body") {
    items.value = appendRound(items.value, {
      card_id: answer.card_id,
      body: answer.body,
      target,
      title: "",
    });
  }

  const remove = (index: number) => (items.value = removeAt(items.value, index));
  const retarget = (index: number, target: AnswerTarget) =>
    (items.value = items.value.map((item, at) => (at === index ? { ...item, target } : item)));
  const title = (index: number, text: string) =>
    (items.value = items.value.map((item, at) => (at === index ? { ...item, title: text } : item)));
  const move = (index: number, delta: number) => (items.value = moveAt(items.value, index, delta));
  const amend = (index: number, body: string) => (items.value = amendAt(items.value, index, body));

  /** 这一轮不要了 */
  function clear() {
    items.value = [];
    previewing.value = false;
  }

  /** 一轮问够了：进预览（**还没落**） */
  const finish = () => (previewing.value = true);
  /** 还想再问几条 */
  const back = () => (previewing.value = false);

  /**
   * 落到这一章：**先插字（一次编辑），再让核心做账**。
   *
   * 返回空串＝成了；否则是错误码（面板拿去显示）。没打开章就落不了——
   * 按钮那边也会按 `canLand` 变灰，这里是第二道。
   */
  async function landAll(at: LandAt): Promise<string> {
    const node = options.currentNode();
    const work = options.workId();
    if (node === null || work === null) return "answer.land_node_invalid";
    if (!hasText.value) return "round.empty";
    // 只有"落正文"的那几条进编辑器；章纲与场景卡由核心写（见 core 那边的落章）
    if (roundHasBody(items.value) && !options.session.insertText(roundText(items.value), at)) {
      return "answer.land_node_invalid";
    }
    try {
      const done = await questionApplyRound(work, node, items.value);
      options.onBoard(await options.settle(done.board));
      options.onLanded?.(done.landed);
      clear();
      return "";
    } catch (error) {
      return asError(error).code;
    }
  }

  return {
    items,
    previewing,
    count,
    hasText,
    collect,
    remove,
    retarget,
    title,
    move,
    amend,
    clear,
    finish,
    back,
    landAll,
  };
}
