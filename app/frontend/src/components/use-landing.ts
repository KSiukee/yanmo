// 「答一条落一条」的那一半：落点、补落、以及"插字 + 留痕"这一对动作。
//
// 与「先问后排版」（`use-round.ts`）是两半：这一半是**就地落**（边想边写的模式 A、
// 或者列表里顺手落一条），那一半是"先攒着、一轮问完一起落"。
//
// 两条路共用同一件事：**正文那几段字由界面插进编辑会话**（于是它就是一次正常编辑，
// 自动落盘、字数、账本、版本快照全照常），**核心只做账**（记下这一条用掉了、落到哪一章）。
// 所以"插字"与"留痕"这两个动作都在这一个文件里，别的文件不各写一份。
import { computed, ref } from "vue";

import { t } from "../locales/index.ts";
import { asError } from "../api/errors.ts";
import { questionLandAnswer, type Answer, type QuestionBoard } from "../api/question.ts";
import type { EditorSession } from "../editor/session.ts";
import { landLabel, type LandAt } from "./question.ts";

export interface LandingOptions {
  session: EditorSession;
  /** 当前这一章（落点）；没打开章就落不了 */
  currentNode: () => number | null;
  onBoard: (board: QuestionBoard) => void;
  /** 落完之后按面板那套口径再读一次（模式 A 会带上"这一章优先"） */
  settle: (board: QuestionBoard) => Promise<QuestionBoard>;
}

export function useLanding(options: LandingOptions) {
  /** 落点与"要不要落进正文"：作者的选择，这次会话内接着用 */
  const landAt = ref<LandAt>("cursor");
  const landToBody = ref(false);
  /** 刚答下的那一份（回执）：答完没落的话，还能回头落一次 */
  const lastAnswer = ref<Answer | null>(null);
  /** 那份答案是在哪一章答下的——切了章就不该再往"现在这一章"落 */
  const lastAnswerNode = ref<number | null>(null);

  const landHint = computed(() => t("flow.land.undo_hint", { at: landLabel(landAt.value) }));
  /** 回执里那条答案还能不能落：还没落过、而且就是**这一章**答下的（切了章就不许乱落） */
  const landableAnswer = computed(
    () =>
      options.currentNode() !== null &&
      lastAnswer.value !== null &&
      lastAnswer.value.status !== "landed" &&
      lastAnswerNode.value === options.currentNode(),
  );

  const setLandAt = (at: LandAt) => (landAt.value = at);
  const setLandToBody = (on: boolean) => (landToBody.value = on);

  /**
   * 进"跟章走"那两种模式时的落法默认：勾上落进正文；给了 `at` 就顺手把落点也挪过去
   * （进「先问后排版」时给章末——那一章通常还是空的，段自然接在后面）。
   */
  function preferLanding(at?: LandAt) {
    landToBody.value = true;
    if (at) landAt.value = at;
  }

  /** 记住刚答下的那一份（回执要用；它同时也是"还能补落"的那一条） */
  function noteAnswer(answer: Answer) {
    lastAnswer.value = answer;
    lastAnswerNode.value = options.currentNode();
  }

  /**
   * 落一条：**先插字（作者的正常编辑），再留痕**。
   *
   * 顺序是刻意的：插字是作者马上看得见的动作，留痕是账。留痕那一步失败时字已经在稿子里了——
   * 如实报错比偷偷撤掉更对（撤掉会把作者刚看见的那一段又抽走）。
   */
  async function landOne(cardId: number, body: string, node: number | null): Promise<boolean> {
    if (node === null || !options.session.insertText(body, landAt.value)) return false;
    options.onBoard(await options.settle(await questionLandAnswer(cardId, node)));
    return true;
  }

  /** 回执里那个「落进本章正文」：答完当时没落，回头还能落一次。 */
  async function landLast(): Promise<{ message: string; errorCode: string }> {
    const answer = lastAnswer.value;
    const node = lastAnswerNode.value;
    if (!answer || node === null || node !== options.currentNode()) return { message: "", errorCode: "" };
    try {
      if (await landOne(answer.card_id, answer.body, node)) {
        lastAnswer.value = { ...answer, status: "landed" };
        return { message: t("flow.land.done", { body: answer.body }), errorCode: "" };
      }
      return { message: t("flow.answer.land_failed", { body: answer.body }), errorCode: "" };
    } catch (error) {
      return { message: "", errorCode: asError(error).code };
    }
  }

  return {
    landAt,
    landToBody,
    lastAnswer,
    landableAnswer,
    landHint,
    setLandAt,
    setLandToBody,
    preferLanding,
    noteAnswer,
    landOne,
    landLast,
  };
}
