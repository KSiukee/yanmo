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
  body: string;
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
  open_deferrals: Deferral[];
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

/** 只看一眼面板（不写库）。 */
export const questionBoard = (work_id: number) =>
  call<QuestionBoard>(COMMANDS.questionBoard, { work_id });

/** 问出这一张（新颖度从这一刻开始算）。 */
export const questionAsk = (card_id: number) =>
  call<QuestionBoard>(COMMANDS.questionAsk, { card_id });

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

/** 按来源静音 / 解除。 */
export const questionMuteSource = (work_id: number, source: string) =>
  call<QuestionBoard>(COMMANDS.questionMuteSource, { work_id, source });
export const questionUnmuteSource = (work_id: number, source: string) =>
  call<QuestionBoard>(COMMANDS.questionUnmuteSource, { work_id, source });

/** 把条件已满足的延后放回候选池（低频：打开面板 / 每几分钟）。 */
export const questionRequeueDue = (work_id: number) =>
  call<QuestionBoard>(COMMANDS.questionRequeueDue, { work_id });
