// 叩问面板的**状态与命令编排**：挑什么来问、问出之后怎么处置、答案怎么落进正文。
//
// 单独成文件的原因（与 `editor/session.ts` 同一条理由）：这一整块是"会发生什么"，
// 而 `.vue` 那一份是"长什么样"——两者的变化理由不一样，混在一个文件里只会越长越难改。
// 组件那一层于是**一个 API 都不直接调**：要做什么都从这里拿。
//
// 四条纪律（与核心一致）：
// 1. **点开一张才算"问出"**——那一刻才消耗新颖度；只看列表不算已问；
// 2. **跟着这一章走**（模式 A）：开着时核心把与当前章有关的问题排最前，答完自动出下一张；
//    排序规则在核心（选题是机制），界面只把当前章报上去——不在这边重排一遍；
// 3. 落进正文的永远是**作者自己的字**：界面的活儿是把它插进编辑会话（于是自动落盘、
//    字数、账本、版本快照全照常），核心只负责记下"这一条用掉了、落到哪一章"；
// 4. 界面不拼句子：模板句从字典渲染，核心只给键与槽位。
import { computed, onMounted, onUnmounted, ref, watch } from "vue";

import { t } from "../locales/index.ts";
import { asError } from "../api/errors.ts";
import {
  questionAnswer,
  questionAsk,
  questionBoard,
  questionDefer,
  questionDiscard,
  questionDrafts,
  questionInspire,
  questionLandAnswer,
  questionMuteClass,
  questionMuteSource,
  questionPraise,
  questionRequeueDue,
  questionRetrieve,
  questionSync,
  questionUndefer,
  questionUnmuteClass,
  questionUnmuteSource,
  type Answer,
  type QuestionBoard,
  type SelectedQuestion,
} from "../api/question.ts";
import type { EditorSession } from "../editor/session.ts";
import { countForChapter, inputLabel, landLabel, renderDraft, type LandAt } from "./question.ts";

/** 面板要的那点外部东西：哪本书，以及编辑会话（落章要用它的正文那条路）。 */
export interface FlowPanelOptions {
  workId: number | null;
  session: EditorSession;
}

export function useFlowPanel(props: FlowPanelOptions) {
  const board = ref<QuestionBoard | null>(null);
  /** 正在问的那一张（点开之后）：它已经离开候选池，面板留着它把处置做完 */
  const active = ref<SelectedQuestion | null>(null);
  const busy = ref(false);
  const errorCode = ref("");
  const deferFor = ref(false);
  const inspireFor = ref(false);
  const answerFor = ref(false);
  const justSaved = ref("");
  /** 模式 A：跟着这一章走（**不落盘**——它是"当下这一会儿"的写法，不是长期偏好） */
  const followChapter = ref(false);
  /** 落点与"要不要落进正文"：作者的选择，这次会话内接着用 */
  const landAt = ref<LandAt>("cursor");
  const landToBody = ref(false);
  /** 刚答下的那一份（回执）：答完没落的话，还能回头落一次 */
  const lastAnswer = ref<Answer | null>(null);
  /** 那份答案是在哪一章答下的——切了章就不该再往"现在这一章"落 */
  const lastAnswerNode = ref<number | null>(null);

  let timer: number | undefined;

  /** 当前这一章（落章的落点、模式 A 的出题都认它）。 */
  const currentChapter = computed(() => props.session.directory.current.value);
  /** 有没有打开着的一章：没有的话落章那两个控件按不了 */
  const canLand = computed(() => currentChapter.value !== null);

  const selected = computed(() => board.value?.selected ?? []);
  const cooled = computed(() => board.value?.cooled ?? []);
  const waiting = computed(() => board.value?.open_deferrals ?? []);
  const mutedSources = computed(() => board.value?.muted_sources ?? []);
  const mutedClasses = computed(() => board.value?.muted_classes ?? []);
  /** 当前这一章叫什么（面板上那句"跟着哪一章走"要说清对象） */
  const chapterName = computed(() => {
    const title = props.session.chapterTitle.value;
    return title.trim() === "" ? t("flow.follow.untitled") : title;
  });
  /** 眼下这份候选里，与当前章有关的有几条 */
  const forChapter = computed(() => countForChapter(selected.value, currentChapter.value));
  /** 回执里那条答案还能不能落：还没落过、而且就是**这一章**答下的（切了章就不许乱落） */
  const landableAnswer = computed(
    () =>
      canLand.value &&
      lastAnswer.value !== null &&
      lastAnswer.value.status !== "landed" &&
      lastAnswerNode.value === currentChapter.value,
  );
  const landHint = computed(() => t("flow.land.undo_hint", { at: landLabel(landAt.value) }));

  /** 报错只给码：句子在字典里（界面文案只有一处来源，别在这里另拼中文）。 */
  function report(error: unknown) {
    errorCode.value = asError(error).code;
  }

  /**
   * 收口：模式 A 开着就再按「这一章优先」读一次面板。
   *
   * 排序规则在核心（选题是机制），界面只把当前章报上去。关着模式 A、
   * 或没打开着任何一章时原样返回，省一次往返。
   */
  async function settle(result: QuestionBoard): Promise<QuestionBoard> {
    const work = props.workId;
    const node = currentChapter.value;
    if (!followChapter.value || !work || node === null) return result;
    return questionBoard(work, node);
  }

  /** 一次完整的刷新：先把到条件的延后放回来，再把新草稿落成卡，最后取最新面板。 */
  async function refresh() {
    const work = props.workId;
    if (!work) {
      board.value = null;
      return;
    }
    try {
      busy.value = true;
      errorCode.value = "";
      let next = await questionRequeueDue(work);
      const drafts = await questionDrafts(work);
      if (drafts.length > 0) {
        // 句子由这里渲染（核心零文案），渲染好再交给核心落卡；同模板同锚点不会重复造
        next = await questionSync(
          work,
          drafts.map((draft) => ({
            template_key: draft.template_key,
            body: renderDraft(draft),
            anchors: draft.anchors,
            importance: draft.importance,
          })),
        );
      }
      board.value = await settle(next);
      if (
        active.value &&
        !board.value.selected.some((item) => item.card_id === active.value?.card_id)
      ) {
        active.value = null; // 处置完就退出"正在问"
      }
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  /** 处置这一类动作的统一收尾：调命令、吃回结果、清掉临时面板。 */
  async function dispose(action: () => Promise<QuestionBoard>) {
    try {
      busy.value = true;
      errorCode.value = "";
      board.value = await settle(await action());
      active.value = null;
      deferFor.value = false;
      inspireFor.value = false;
      answerFor.value = false;
      justSaved.value = "";
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  async function ask(card: SelectedQuestion) {
    try {
      busy.value = true;
      errorCode.value = "";
      board.value = await questionAsk(card.card_id);
      active.value = card;
      answerFor.value = false;
      justSaved.value = "";
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  /** 模式 A 的「下一张」：候选池里最前面那张（刚答下的那张已经不在池子里了）。 */
  async function askNext() {
    const next = selected.value[0];
    if (next) await ask(next);
  }

  /** 跳过这张：不处置、不落章，直接看下一张（停在候选池里，下轮还会被问到）。 */
  async function skip() {
    active.value = null;
    await askNext();
  }

  /** 打开作答框：把上一次那张的回执清掉，免得"答下了：…"跟着新的一张走。 */
  function openAnswer() {
    answerFor.value = true;
    justSaved.value = "";
  }

  /**
   * 作答：答案进答案池，问题卡就此走到终态。
   *
   * `source` 现在只有 `typed`（键盘）；口述那条链路落地后，那条路把 `voice` / `mixed`
   * 传进来就行——命令与存储的形状都不变，这正是"文本与输入方式解耦"要的效果。
   *
   * 勾了「落进正文」的话，落章分两半走：**界面把字插进编辑会话**（于是自动落盘、字数、
   * 账本、版本快照全照常），**核心记下"这一条用掉了、落到哪一章"**——两边各做各的那一半。
   */
  async function saveAnswer(body: string) {
    const card = active.value;
    if (!card || !body.trim()) return;
    try {
      busy.value = true;
      errorCode.value = "";
      // 这一版只有键盘这一条通道；口述那条链路落地后，这里换成 voice / mixed 就行
      const source = "typed";
      const receipt = await questionAnswer(card.card_id, body, source);
      const node = currentChapter.value;
      lastAnswer.value = receipt.answer;
      lastAnswerNode.value = node;
      board.value = await settle(receipt.board);
      // 回执照**核心落下的那一份**说（不是照界面自己传的那份）：修剪过的原文、认下的输入方式
      let done = t("flow.answer.saved", {
        kind: inputLabel(receipt.answer.source),
        body: receipt.answer.body,
      });
      if (landToBody.value && !(await landAnswer(card.card_id, receipt.answer.body, node))) {
        done = t("flow.answer.land_failed", { body: receipt.answer.body });
      }
      active.value = null;
      answerFor.value = false;
      // 模式 A：答完接着问下一张。回执要写在它**后面**——ask 会把上一张的回执清掉
      if (followChapter.value) await askNext();
      justSaved.value = done;
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  /**
   * 把一条答案落进正文：**先插字（作者的正常编辑），再留痕**。
   *
   * 顺序是刻意的：插字是作者马上看得见的动作，留痕是账。留痕那一步失败时字已经在稿子里了——
   * 如实报错比偷偷撤掉更对（撤掉会把作者刚看见的那一段又抽走）。
   */
  async function landAnswer(cardId: number, body: string, node: number | null): Promise<boolean> {
    if (node === null || !props.session.insertText(body, landAt.value)) return false;
    board.value = await settle(await questionLandAnswer(cardId, node));
    return true;
  }

  /** 回执里那个「落进本章正文」：答完当时没落，回头还能落一次。 */
  async function landLastAnswer() {
    const answer = lastAnswer.value;
    const node = lastAnswerNode.value;
    if (!answer || node === null || node !== currentChapter.value) return;
    try {
      busy.value = true;
      errorCode.value = "";
      if (await landAnswer(answer.card_id, answer.body, node)) {
        lastAnswer.value = { ...answer, status: "landed" };
        justSaved.value = t("flow.land.done", { body: answer.body });
      } else {
        justSaved.value = t("flow.answer.land_failed", { body: answer.body });
      }
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  async function saveInspiration(body: string) {
    const card = active.value;
    if (!card || !body.trim()) return;
    try {
      busy.value = true;
      errorCode.value = "";
      // 口述那条路落地后，这里的 source 换成 voice / mixed
      const idea = await questionInspire(card.card_id, body, "typed");
      justSaved.value = t("flow.inspire.saved", { body: idea.body });
      inspireFor.value = false;
      // **状态不动**：只把面板刷新一遍，正在问的那一张还在
      if (props.workId) board.value = await settle(await questionBoard(props.workId));
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  /** 开关模式 A：换一种出题顺序（重新问一次核心；排序不在这边做） */
  function toggleFollow(event: Event) {
    followChapter.value = (event.target as HTMLInputElement).checked;
    // 模式 A 的意思就是"边想边写、答完落进这一章"：打开它时顺手把落章勾上
    // （作者当然还能自己取消——落不落始终是他定的）
    if (followChapter.value) landToBody.value = true;
    void refresh();
  }

  // ── 处置动作：组件只管点，命令全在这儿调（组件那一层一个 API 都不碰） ──────────
  const openDefer = () => (deferFor.value = true);
  const closeDefer = () => (deferFor.value = false);
  const openInspire = () => (inspireFor.value = true);
  const closeInspire = () => (inspireFor.value = false);
  const closeAnswer = () => (answerFor.value = false);
  const setLandAt = (at: LandAt) => (landAt.value = at);
  const setLandToBody = (on: boolean) => (landToBody.value = on);
  const praise = () => (active.value ? dispose(() => questionPraise(active.value!.card_id)) : undefined);
  const discard = () =>
    active.value ? dispose(() => questionDiscard(active.value!.card_id)) : undefined;
  const muteClass = () =>
    active.value ? dispose(() => questionMuteClass(active.value!.card_id)) : undefined;
  const deferCurrent = (preset: string, note: string) =>
    deferFor.value && active.value
      ? dispose(() => questionDefer(active.value!.card_id, preset, note))
      : undefined;
  const undefer = (cardId: number) => dispose(() => questionUndefer(cardId));
  const retrieve = (cardId: number) => dispose(() => questionRetrieve(cardId));
  const muteSource = (source: string) =>
    props.workId ? dispose(() => questionMuteSource(props.workId!, source)) : undefined;
  const unmuteClass = (key: string) =>
    props.workId ? dispose(() => questionUnmuteClass(props.workId!, key)) : undefined;
  const unmuteSource = (source: string) =>
    props.workId ? dispose(() => questionUnmuteSource(props.workId!, source)) : undefined;

  onMounted(() => {
    void refresh();
    // 低频定时：条件可能在这期间满足（写完了那一章、到了那一天）
    timer = window.setInterval(() => void refresh(), 5 * 60_000);
  });
  onUnmounted(() => {
    if (timer !== undefined) window.clearInterval(timer);
  });
  watch(() => props.workId, () => void refresh());
  // 跟着这一章走：作者切了章，出题顺序也跟着换（重新问一次核心）
  watch(currentChapter, () => {
    if (followChapter.value) void refresh();
  });

  return {
    board,
    active,
    busy,
    errorCode,
    deferFor,
    inspireFor,
    answerFor,
    justSaved,
    followChapter,
    landAt,
    landToBody,
    canLand,
    selected,
    cooled,
    waiting,
    mutedSources,
    mutedClasses,
    chapterName,
    forChapter,
    landableAnswer,
    landHint,
    refresh,
    ask,
    skip,
    openAnswer,
    saveAnswer,
    saveInspiration,
    landLastAnswer,
    toggleFollow,
    openDefer,
    closeDefer,
    openInspire,
    closeInspire,
    closeAnswer,
    setLandAt,
    setLandToBody,
    praise,
    discard,
    muteClass,
    deferCurrent,
    undefer,
    retrieve,
    muteSource,
    unmuteClass,
    unmuteSource,
  };
}
