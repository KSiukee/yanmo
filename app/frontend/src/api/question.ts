// 叩问：候选问题、选题、处置四件套与记灵感（命令名在 `./core` 的白名单里登记）。
//
// 这一层只做"取值 ↔ 参数"的转换；模板池、引力、偏好、延后条件全在核心。
//
// 三个口径（与核心的状态机一致，界面上也照这么说）：
// - **点开一张才算"问出"**：那一刻才消耗新颖度、才进冷却；只看列表不算已问；
// - **记灵感与处置正交**：记完灵感，问题状态一个字节都不动；
// - **舍弃不等于删除**：它进冷却库，随时能捞回来。

import { call, COMMANDS } from "./core";

/** 一条候选**草稿**：模板键 + 槽位取值——**句子里一个字都没有**，由界面按字典渲染。 */
export interface QuestionDraft {
  template_key: string;
  element: "chapter" | "plan" | "rhythm" | "continuity";
  slots: Record<string, string>;
  anchors: string[];
  importance: number;
}

/** 引力的拆解：既用来排序，也用来回答"为什么先问它"。 */
export interface Gravity {
  total: number;
  timeliness: number;
  importance: number;
  novelty: number;
  derived_discount: number;
  template_weight: number;
  defer_penalty: number;
  level: "short" | "mid" | "long";
  cooled: boolean;
}

/** 选出来的一条问题。 */
export interface SelectedQuestion {
  card_id: number;
  template_key: string;
  /** 哪一类要素（`plan` 那一类的答案默认落"章纲"） */
  element: string;
  body: string;
  /** 卡上的关联锚点（`chapter:12` 这种）：界面靠它认"这条问的是不是这一章" */
  anchors: string[];
  gravity: Gravity;
}

/** 冷却库里的一条（舍弃的卡）。 */
export interface CooledCard {
  card_id: number;
  template_key: string;
  body: string;
  source: string;
  cooled_at: number;
}

/** 一条还没重出的延后。 */
export interface Deferral {
  id: number;
  card_id: number;
  kind: "time" | "written" | "manual";
  due_at_ms: number | null;
  anchor_node: number | null;
  note: string;
  created_at: number;
  resolved_at: number | null;
}

/** 面板一屏要的全部数据（一次给全，免得同一屏前后对不上）。 */
export interface QuestionBoard {
  selected: SelectedQuestion[];
  cooled: CooledCard[];
  muted_sources: string[];
  /** 已静音的**类别**（模板键；界面按字典渲染成"这类别再问"） */
  muted_classes: string[];
  open_deferrals: Deferral[];
  /** 候选池里没摆上来的还有多少条（一屏只摆得下几条，同类更是先只摆一条） */
  more: number;
}

/** 记下的那张灵感卡（带溯源：它从哪张问题卡来）。 */
export interface Inspiration {
  id: number;
  work_id: number;
  body: string;
  source: string;
  derived_from: number | null;
  created_at: number;
}

/** 答下的那张答案卡（与灵感卡同表，`card_id` 指回问题卡）。 */
export interface Answer {
  id: number;
  work_id: number;
  card_id: number;
  body: string;
  /** 怎么打出来的：`typed` / `voice` / `mixed`（用 `inputLabel` 讲成人话） */
  source: string;
  /** `pending` = 躺在答案池里；`landed` = 落进过正文（见 `questionLandAnswer`） */
  status: string;
  created_at: number;
}

/** 作答的回执：**核心落下来的那一条** + 落完之后的最新面板。 */
export interface AnswerReceipt {
  answer: Answer;
  board: QuestionBoard;
}

/** 一条答案落到哪儿：正文段落 / 章纲（这一章的一句话）/ 场景卡（这一章下面新建一张）。 */
export type AnswerTarget = "body" | "outline" | "scene";

/** 一轮里的一条（先问后排版）：答的是哪张卡、作者最终认定的那段字、落到哪儿。 */
export interface RoundItem {
  card_id: number;
  /** 作者在托盘里可能改过；与库里不同时核心回写答案池并留痕 */
  body: string;
  /** 空串＝正文（老调用不带这一栏）；认不出的核心当场拒 */
  target: AnswerTarget | "";
  /** 场景卡的名字；空着留给作者在树上起名 */
  title: string;
}

/** 一次落章的结果（核心记下的账）。 */
export interface LandReceipt {
  answer_ids: number[];
  /** 新建的场景卡 id（目录树要重拉才看得见） */
  scene_ids: number[];
  /** 章纲最后成了什么（没落章纲就是 null）——拿它更新"一句话"那一栏 */
  outline: string | null;
}

/** 落章回执：账 + 落完之后的最新面板。 */
export interface LandDone {
  landed: LandReceipt;
  board: QuestionBoard;
}

/** 界面渲染好的一条候选（核心只认键与槽位，句子是界面按字典填出来的）。 */
export interface QuestionOffer {
  template_key: string;
  body: string;
  anchors: string[];
  importance: number;
}

/** 候选草稿（不含句子）。 */
export const questionDrafts = (work_id: number) =>
  call<QuestionDraft[]>(COMMANDS.questionDrafts, { work_id });

/** 把界面渲染好的问题落成卡（同模板同锚点不会重复造），并回一次最新面板。 */
export const questionSync = (work_id: number, offers: QuestionOffer[]) =>
  call<QuestionBoard>(COMMANDS.questionSync, { work_id, offers });

/**
 * 看一眼面板（不写库）。
 *
 * 给了 `node_id` 就按**「这一章优先」**排序（模式 A 用）：与这一章有关的问题排最前，
 * 其余照旧按引力跟着。排序规则在核心——界面只把当前章报上去。
 */
export const questionBoard = (work_id: number, node_id: number | null = null) =>
  call<QuestionBoard>(COMMANDS.questionBoard, { work_id, node_id });

/** 问出这一张（新颖度从这一刻开始算）。 */
export const questionAsk = (card_id: number) =>
  call<QuestionBoard>(COMMANDS.questionAsk, { card_id });

/**
 * 作答：答案落进答案池，问题卡走到终态。
 *
 * `source` 是**怎么打出来的**（`typed` / `voice` / `mixed`）——文本与输入方式解耦：
 * 口述那条链路落地后，这里换成 `voice` / `mixed` 就行，命令形状不变。
 * 回执里那条答案是**核心读回来的**（修剪过的原文 + 核心认下的输入方式），不是界面自己回显。
 */
export const questionAnswer = (card_id: number, body: string, source: string) =>
  call<AnswerReceipt>(COMMANDS.questionAnswer, { card_id, body, source });

/** 延后：`preset` 是界面那一档的键，`note` 是作者自己填的那一句。 */
export const questionDefer = (card_id: number, preset: string, note: string) =>
  call<QuestionBoard>(COMMANDS.questionDefer, { card_id, preset, note });

/** 舍弃：进冷却库（可捞回，同时教系统少问这类）。 */
export const questionDiscard = (card_id: number) =>
  call<QuestionBoard>(COMMANDS.questionDiscard, { card_id });

/** 说「这个问题好」：状态不动，只教同类模板。 */
export const questionPraise = (card_id: number) =>
  call<QuestionBoard>(COMMANDS.questionPraise, { card_id });

/** 永久静音这一类。 */
export const questionMuteClass = (card_id: number) =>
  call<QuestionBoard>(COMMANDS.questionMuteClass, { card_id });

/** 从冷却库捞回。 */
export const questionRetrieve = (card_id: number) =>
  call<QuestionBoard>(COMMANDS.questionRetrieve, { card_id });

/** 记灵感：不动问题状态。 */
export const questionInspire = (card_id: number, body: string, source: string) =>
  call<Inspiration>(COMMANDS.questionInspire, { card_id, body, source });

/**
 * 落章：把一条答案标成「落进过正文」（**只留痕，不写正文**）。
 *
 * 正文那一段字由调用方插进编辑会话（`session.insertText`）——那是作者的一次正常编辑，
 * 自动落盘、字数、账本、版本快照全照常。这里只记下"这一条用掉了、落到哪一章"。
 */
export const questionLandAnswer = (
  card_id: number,
  node_id: number,
  target: AnswerTarget | "" = "body",
  title = "",
) => call<LandDone>(COMMANDS.questionLandAnswer, { card_id, node_id, target, title });

/**
 * 一轮落章（先问后排版）：把这一轮攒下的答案**一次**落进这一章。
 *
 * 正文那几段字由调用方**一次**插进编辑会话（`session.insertText`，与单条落章同一条路）；
 * 这里让核心做账：回写作者改过的字、把每条标成落过、逐条留痕（一个事务）。
 * 顺序就是 `items` 的顺序——作者在托盘里排的那个。
 */
export const questionApplyRound = (work_id: number, node_id: number, items: RoundItem[]) =>
  call<LandDone>(COMMANDS.questionApplyRound, { work_id, node_id, items });

/** 解除**这一类**的静音（"这类别再问"的回头路）。 */
export const questionUnmuteClass = (work_id: number, template_key: string) =>
  call<QuestionBoard>(COMMANDS.questionUnmuteClass, { work_id, template_key });

/** **别等了**：取消延后、当场回候选池。 */
export const questionUndefer = (card_id: number) =>
  call<QuestionBoard>(COMMANDS.questionUndefer, { card_id });

/** 按来源静音 / 解除。 */
export const questionMuteSource = (work_id: number, source: string) =>
  call<QuestionBoard>(COMMANDS.questionMuteSource, { work_id, source });
export const questionUnmuteSource = (work_id: number, source: string) =>
  call<QuestionBoard>(COMMANDS.questionUnmuteSource, { work_id, source });

/** 把条件已满足的延后放回候选池（低频：打开面板 / 每几分钟）。 */
export const questionRequeueDue = (work_id: number) =>
  call<QuestionBoard>(COMMANDS.questionRequeueDue, { work_id });
