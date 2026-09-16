// 叩问面板的**状态与命令编排**：挑什么来问、问出之后怎么处置、走法怎么切换。
//
// 单独成文件的原因（与 `editor/session.ts` 同一条理由）：这一整块是"会发生什么"，
// 而 `.vue` 那一份是"长什么样"；组件那一层于是**一个 API 都不直接调**。
//
// 三种走法（面板顶上那两个开关，同时只能开一种）：**列表**（默认，随手答）/
// **跟着这一章走**（模式 A：答完自动出下一张，答一条落一条）/ **先问后排版**
// （模式 B：答案先攒在托盘里，问够了一起落）。
//
// 三条纪律（与核心一致）：点开一张才算"问出"；排序在核心（界面只报当前章）；
// 落进正文的永远是作者自己的字（界面插进编辑会话、核心记账）。
import { computed, onMounted, onUnmounted, ref, watch } from "vue";

import { t } from "../locales/index.ts";
import { asError } from "../api/errors.ts";
import {
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
  type LandReceipt,
  type QuestionBoard,
  type SelectedQuestion,
} from "../api/question.ts";
import type { EditorSession } from "../editor/session.ts";
import { asTone, countForChapter, renderDraft } from "./question.ts";
import { useAnswerFlow } from "./use-answer-flow.ts";
import { useLanding } from "./use-landing.ts";
import { useRound } from "./use-round.ts";

/** 面板的三种走法（列表 / 跟着这一章走 / 先问后排版）。 */
export type PanelMode = "list" | "walk" | "round";

/** 面板要的那点外部东西：哪本书，以及编辑会话（落章要用它的正文那条路）。 */
export interface FlowPanelOptions {
  workId: number | null;
  session: EditorSession;
  /** 推过来的那张卡（作者点了提示条上的「答一句」）：直接在「正在问」区打开 */
  openQuestion?: SelectedQuestion | null;
}

export function useFlowPanel(props: FlowPanelOptions, hooks: { onPushedOpened?: () => void } = {}) {
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

  /** 现在生效的语气（跟书走；没打开书就用全局那份）——句子的三版按它挑 */
  const tone = computed(() =>
    asTone(
      (props.session.appearance.workValues.value ?? props.session.appearance.values.value)
        ?.question_tone,
    ),
  );

  const followChapter = computed(() => mode.value !== "list");
  const roundMode = computed(() => mode.value === "round");

  /** 核心记下的账回到界面（更新的是**库里的真值**，不是界面自己猜的）：
   * 章纲落了就把"一句话"换成核心给的那一行；新建了场景卡就重拉目录树（新卡要看得见）。 */
  function onLanded(landed: LandReceipt) {
    if (landed.outline !== null) props.session.note.reset(landed.outline);
    if (landed.scene_ids.length > 0) void props.session.directory.refresh();
  }

  // 两半落法各自成件：就地落（useLanding）与一轮落（useRound）。它们都要"落完之后换个面板"
  // 与"按面板口径再读一次"，所以把那几件事传进去。
  const landing = useLanding({
    session: props.session,
    currentNode: () => currentChapter.value,
    onBoard: (next) => (board.value = next),
    settle,
    onLanded,
  });
  const round = useRound({
    session: props.session,
    workId: () => props.workId,
    currentNode: () => currentChapter.value,
    onBoard: (next) => (board.value = next),
    settle,
    onLanded,
  });

  // 答一条之后的整条链路（存 → 攒或落 → 回执 → 下一张）单独成件，见 use-answer-flow.ts
  const { saveAnswer } = useAnswerFlow({
    session: props.session,
    active,
    busy,
    errorCode,
    justSaved,
    answerFor,
    roundMode,
    followChapter,
    currentChapter,
    landing,
    round,
    setBoard: (next) => (board.value = next),
    settle,
    askNext,
    report,
  });

  /** 报错只给码：句子在字典里（界面文案只有一处来源）。 */
  function report(error: unknown) {
    errorCode.value = asError(error).code;
  }

  /** 收口：跟着这一章走时再按「这一章优先」读一次面板（排序规则在核心，界面只报当前章）。
   * 不跟章、或没打开着任何一章时原样返回，省一次往返。 */
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
            body: renderDraft(draft, tone.value),
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

  /** 换走法：三种共用一个开关位（同时只有一种）。落法默认交给 `useLanding` 那边定。 */
  function pickMode(next: PanelMode, on: boolean) {
    mode.value = on ? next : "list";
    if (mode.value === "round") landing.preferLanding("end");
    else if (mode.value === "walk") landing.preferLanding();
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
  const withWork = (action: (workId: number, arg: string) => Promise<QuestionBoard>) =>
    (arg: string) => (props.workId ? dispose(() => action(props.workId!, arg)) : undefined);
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
  // 推过来的那张卡：直接在「正在问」区打开（它已经被核心算过"已问"了，
  // 这里只是把它摆出来；打开之后回报一声，父层把那次交接清掉）
  watch(
    () => props.openQuestion,
    (card) => {
      if (!card) return;
      active.value = card;
      answerFor.value = false;
      justSaved.value = "";
      hooks.onPushedOpened?.();
    },
  );
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
