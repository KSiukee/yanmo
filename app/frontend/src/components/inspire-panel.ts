// 创作流面板里「灵感」那一片的**状态与命令编排**：排出来、采纳、丢弃、撤销。
//
// 单独成文件的原因与 `creator-panel.ts` 同一条：这一整块是"会发生什么"，
// `.vue` 那一份是"长什么样"，组件那一层于是**一个 API 都不直接调**。
//
// 三条分寸（与核心一致）：
// - **先插字、再记账**：落正文时作者先看见字进了稿子，之后再记这笔账（与叩问落答案
//   同一条纪律）——记账失败时字已经在稿子里了，如实报错比偷偷撤掉更对；
// - **不做它的主**：这一屏只排序，一个字都不生成；采纳到哪儿、要不要采纳由作者定；
// - **丢弃是软删**：所以"撤销"当场就能兑现。
//
// 与碎片那一片的一处刻意不同：**这一片是按需读的**（组件挂上、或切了章才读一次），
// 不在会话启动时先打一次库——不打开这一片的人不为它付一次读。

import { computed, ref, type Ref } from "vue";

import { asError } from "../api/errors.ts";
import {
  inspireAdopt,
  inspireBoard,
  inspireDiscard,
  type InspireBoard,
  type Spark,
  type SparkTarget,
} from "../api/inspire.ts";
import { fragmentRestore } from "../api/fragment.ts";
import { t } from "../locales/index.ts";
import { sceneTitleFrom, targetLabel } from "./inspire.ts";

/** 这一片要的那点外部东西：哪本书、现在哪一章，以及往编辑会话里插字的把手。 */
export interface InspirePanelOptions {
  workId: Ref<number | null>;
  /** 当前这一章：**排序要看它**（跟这一章有关的排前面），落章纲 / 场景卡也要它 */
  currentChapter: Ref<number | null>;
  /** 落正文时把字插进编辑会话（与叩问落答案同一条路；核心不碰编辑器手里那一份） */
  insertText: (text: string, at: "cursor" | "end") => boolean;
}

export interface InspirePanelState {
  board: Ref<InspireBoard | null>;
  busy: Ref<boolean>;
  /** 失败时那句**已经渲染好的**话（`CoreError` 走字典渲染，界面不拼中文也不露码） */
  errorText: Ref<string>;
  /** 刚办成的那件事（一句话回执） */
  receipt: Ref<string>;
  /** 眼下选了哪一档落点（**不落盘**：它是"这次想送到哪儿"，不是长期偏好） */
  target: Ref<SparkTarget>;
  /** 落点闭集（**核心给的**，界面不自己写一份） */
  targets: Ref<SparkTarget[]>;
  list: Ref<Spark[]>;
  /** 这一档落点要不要先打开一章（正文 / 章纲 / 场景卡要；事件池不要） */
  needsChapter: Ref<boolean>;
  /** 现在能不能采纳：有书，且（这一档不需要章，或确实打开着章） */
  canAdopt: Ref<boolean>;
  currentChapter: Ref<number | null>;
  /** 刚丢弃的那一条（撤销要认它） */
  lastDiscarded: Ref<number | null>;
  refresh: () => Promise<void>;
  setTarget: (target: SparkTarget) => void;
  adopt: (spark: Spark) => Promise<void>;
  discard: (spark: Spark) => Promise<void>;
  undoDiscard: () => Promise<void>;
}

export function useInspirePanel(deps: InspirePanelOptions): InspirePanelState {
  const board = ref<InspireBoard | null>(null);
  const busy = ref(false);
  const errorText = ref("");
  const receipt = ref("");
  const target = ref<SparkTarget>("body");
  const lastDiscarded = ref<number | null>(null);

  const targets = computed(() => board.value?.targets ?? []);
  const list = computed(() => board.value?.sparks ?? []);
  const currentChapter = computed(() => deps.currentChapter.value);
  const needsChapter = computed(() => target.value !== "event");
  const canAdopt = computed(
    () => deps.workId.value !== null && (!needsChapter.value || currentChapter.value !== null),
  );

  /** 报错：把 `CoreError` 渲染好的那一句直接摆出来（它本来就是按字典拼的）。 */
  function report(error: unknown) {
    errorText.value = asError(error).message;
  }

  async function refresh() {
    const work = deps.workId.value;
    if (!work) {
      board.value = null;
      return;
    }
    try {
      busy.value = true;
      errorText.value = "";
      board.value = await inspireBoard(work, currentChapter.value);
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  function setTarget(next: SparkTarget) {
    target.value = next;
    errorText.value = "";
  }

  /**
   * 采纳一条。
   *
   * 落正文：**先插字、再记账**（插字失败就压根不记这笔账——字没进去，账不该认）。
   * 落章纲 / 场景卡 / 事件池：字不进编辑器，由核心一个事务写（低频的结构改动）。
   */
  async function adopt(spark: Spark) {
    if (!canAdopt.value) {
      errorText.value = t("inspire.need_chapter");
      return;
    }
    const chosen = target.value;
    try {
      busy.value = true;
      errorText.value = "";
      receipt.value = "";
      if (chosen === "body" && !deps.insertText(spark.body, "cursor")) {
        errorText.value = t("inspire.need_chapter");
        return;
      }
      const done = await inspireAdopt(
        spark.id,
        chosen,
        currentChapter.value,
        chosen === "scene" ? sceneTitleFrom(spark.body) : "",
      );
      // 采纳之后这一屏该是什么，由核心回给（界面不自己猜少了哪一条、顺序变了没）
      board.value = board.value ? { ...board.value, sparks: done.sparks } : null;
      if (!board.value) await refresh();
      lastDiscarded.value = null;
      receipt.value = t("inspire.adopted", { target: targetLabel(done.target) });
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  /** 丢弃一条（软删）：撤销要认它，所以把 id 留着。 */
  async function discard(spark: Spark) {
    try {
      busy.value = true;
      errorText.value = "";
      receipt.value = "";
      board.value = board.value
        ? { ...board.value, sparks: await inspireDiscard(spark.id, currentChapter.value) }
        : null;
      lastDiscarded.value = spark.id;
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  /** 撤销刚才那一次丢弃（捞回走碎片那条现成的路：软删抹掉时间戳）。 */
  async function undoDiscard() {
    const id = lastDiscarded.value;
    if (id === null) return;
    try {
      busy.value = true;
      errorText.value = "";
      await fragmentRestore(id);
      lastDiscarded.value = null;
      await refresh();
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  return {
    board,
    busy,
    errorText,
    receipt,
    target,
    targets,
    list,
    needsChapter,
    canAdopt,
    currentChapter,
    lastDiscarded,
    refresh,
    setTarget,
    adopt,
    discard,
    undoDiscard,
  };
}
