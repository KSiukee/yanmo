// 叩问面板的**状态与命令编排**：挑什么来问、问出之后怎么处置、走法怎么切换。
//
// 单独成文件的原因（与 `editor/session.ts` 同一条理由）：这一整块是"会发生什么"，
// 而 `.vue` 那一份是"长什么样"——两者的变化理由不一样。组件那一层于是**一个 API 都不直接调**。
//
// 三种走法（面板顶上那两个开关，同时只能开一种）：
// - **列表**（默认）：按引力排一排，你挑着答；答完就地落（想落的话）——随手用；
// - **跟着这一章走**（模式 A）：当前章的问题排最前，答完自动出下一张，答一条落一条——边想边写；
// - **先问后排版**（模式 B）：同样跟着这一章走，但答案**先攒着**，一轮问够了再一起落。
//
// 三条纪律（与核心一致）：点开一张才算"问出"；排序在核心（界面只报当前章）；
// 落进正文的永远是作者自己的字（界面插进编辑会话、核心记账）。
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
  questionMuteClass,
  questionMuteSource,
  questionPraise,
  questionRequeueDue,
  questionRetrieve,
  questionSync,
  questionUndefer,
  questionUnmuteClass,
  questionUnmuteSource,
  type QuestionBoard,
  type SelectedQuestion,
} from "../api/question.ts";
import type { EditorSession } from "../editor/session.ts";
import { countForChapter, inputLabel, renderDraft } from "./question.ts";
import { useLanding } from "./use-landing.ts";
import { useRound } from "./use-round.ts";

/** 面板的三种走法（列表 / 跟着这一章走 / 先问后排版）。 */
export type PanelMode = "list" | "walk" | "round";

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
  /** 走法（**不落盘**——它是"当下这一会儿"的写法，不是长期偏好） */
  const mode = ref<PanelMode>("list");

  let timer: number | undefined;

  /** 当前这一章（落章的落点、跟章出题都认它）。 */
  const currentChapter = computed(() => props.session.directory.current.value);
  /** 有没有打开着的一章：没有的话落章那些控件按不了 */
  const canLand = computed(() => currentChapter.value !== null);

  const selected = computed(() => board.value?.selected ?? []);
  /** 当前这一章叫什么（面板上那句"跟着哪一章走"要说清对象） */
  const chapterName = computed(() => {
    const title = props.session.chapterTitle.value;
    return title.trim() === "" ? t("flow.follow.untitled") : title;
  });
  /** 眼下这份候选里，与当前章有关的有几条 */
  const forChapter = computed(() => countForChapter(selected.value, currentChapter.value));

  const followChapter = computed(() => mode.value !== "list");
  const roundMode = computed(() => mode.value === "round");

  // 两半落法各自成件：就地落（useLanding）与一轮落（useRound）。它们都要"落完之后换个面板"
  // 与"按面板口径再读一次"，所以把那两件事传进去。
  const landing = useLanding({
    session: props.session,
    currentNode: () => currentChapter.value,
    onBoard: (next) => (board.value = next),
    settle,
  });
  const round = useRound({
    session: props.session,
    workId: () => props.workId,
    currentNode: () => currentChapter.value,
    onBoard: (next) => (board.value = next),
    settle,
  });

  /** 报错只给码：句子在字典里（界面文案只有一处来源，别在这里另拼中文）。 */
  function report(error: unknown) {
    errorCode.value = asError(error).code;
  }

  /**
   * 收口：跟着这一章走时再按「这一章优先」读一次面板。
   *
   * 排序规则在核心（选题是机制），界面只把当前章报上去。不跟章、或没打开着任何一章时
   * 原样返回，省一次往返。
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

  /** 跟着章走时的「下一张」：候选池里最前面那张（刚答下的那张已经不在池子里了）。 */
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
   * 落不落、怎么落，看当前走法：
   * - **先问后排版**：攒进这一轮（先不落），一轮问够了再一起落；
   * - 其余：勾了「落进正文」就**就地落**（答复制里还能补落）。
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
      // 回执照**核心落下的那一份**说（不是照界面自己传的那份）：修剪过的原文、认下的输入方式
      let done = t("flow.answer.saved", {
        kind: inputLabel(receipt.answer.source),
        body: receipt.answer.body,
      });
      if (roundMode.value) {
        round.collect(receipt.answer);
      } else {
        landing.noteAnswer(receipt.answer);
        const missed =
          landing.landToBody.value &&
          !(await landing.landOne(card.card_id, receipt.answer.body, node));
        if (missed) done = t("flow.answer.land_failed", { body: receipt.answer.body });
      }
      board.value = await settle(receipt.board);
      active.value = null;
      answerFor.value = false;
      // 跟章走：答完接着问下一张。回执要写在它**后面**——ask 会把上一张的回执清掉
      if (followChapter.value) await askNext();
      justSaved.value = done;
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  /** 回执里那个「落进本章正文」：答完当时没落，回头还能落一次。 */
  async function landLastAnswer() {
    try {
      busy.value = true;
      errorCode.value = "";
      const outcome = await landing.landLast();
      if (outcome.errorCode) errorCode.value = outcome.errorCode;
      else if (outcome.message) justSaved.value = outcome.message;
    } finally {
      busy.value = false;
    }
  }

  /** 一轮问够了 → 落到这一章（预览见过之后）。 */
  async function landRound() {
    const count = round.count.value;
    try {
      busy.value = true;
      errorCode.value = "";
      const code = await round.landAll(landing.landAt.value);
      if (code) errorCode.value = code;
      else justSaved.value = t("flow.round.done", { count });
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

  /**
   * 换走法。三种走法共用一个开关位（同时只能有一种）。
   *
   * 打开跟章走的那两种时顺手勾上「落进正文」——那两种模式的意思本来就是"答完就落进这一章"
   * （模式 B 只是把"落"推到一轮之后）；作者当然还能自己取消。
   */
  function pickMode(next: PanelMode, on: boolean) {
    mode.value = on ? next : "list";
    if (mode.value !== "list") landing.setLandToBody(true);
    void refresh();
  }
  const toggle = (next: PanelMode) => (event: Event) =>
    pickMode(next, (event.target as HTMLInputElement).checked);
  const toggleWalk = toggle("walk");
  const toggleRound = toggle("round");

  // ── 处置动作：组件只管点，命令全在这儿调。两条都过一道手——处置的对象永远是
  // "正在问的那一张"，回头路那几个则要带上"哪本书"（各自判一遍容易漏，漏了就悄悄点不动）。
  const withActive = (action: (cardId: number) => Promise<QuestionBoard>) =>
    active.value ? dispose(() => action(active.value!.card_id)) : undefined;
  const withWork = (action: (workId: number, arg: string) => Promise<QuestionBoard>) => {
    return (arg: string) =>
      props.workId ? dispose(() => action(props.workId!, arg)) : undefined;
  };
  const openDefer = () => (deferFor.value = true);
  const closeDefer = () => (deferFor.value = false);
  const openInspire = () => (inspireFor.value = true);
  const closeInspire = () => (inspireFor.value = false);
  const closeAnswer = () => (answerFor.value = false);
  const praise = () => withActive(questionPraise);
  const discard = () => withActive(questionDiscard);
  const muteClass = () => withActive(questionMuteClass);
  const deferCurrent = (preset: string, note: string) =>
    deferFor.value ? withActive((cardId) => questionDefer(cardId, preset, note)) : undefined;
  const undefer = (cardId: number) => dispose(() => questionUndefer(cardId));
  const retrieve = (cardId: number) => dispose(() => questionRetrieve(cardId));
  const muteSource = withWork(questionMuteSource);
  const unmuteClass = withWork(questionUnmuteClass);
  const unmuteSource = withWork(questionUnmuteSource);

  onMounted(() => {
    void refresh();
    // 低频定时：条件可能在这期间满足（写完了那一章、到了那一天）
    timer = window.setInterval(() => void refresh(), 5 * 60_000);
  });
  onUnmounted(() => {
    if (timer !== undefined) window.clearInterval(timer);
  });
  watch(() => props.workId, () => void refresh());
  // 跟章走：作者切了章，出题顺序也跟着换（重新问一次核心）
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
    roundMode,
    canLand,
    selected,
    chapterName,
    forChapter,
    // 两半落法（模板直接读它们的 ref / 方法）
    landing,
    round,
    refresh,
    ask,
    skip,
    openAnswer,
    saveAnswer,
    saveInspiration,
    landLastAnswer,
    landRound,
    toggleWalk,
    toggleRound,
    openDefer,
    closeDefer,
    openInspire,
    closeInspire,
    closeAnswer,
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
