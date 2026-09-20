// 碰撞那一屏的**状态与命令编排**：摆出来、写一句存下、跳过、撤销跳过。
//
// 两头可能是碎片，也可能是大纲元素（章纲 / 伏笔 / 人物卡）——这一层不必知道差别：
// 核心给的是**规范标识**（`spark:12` / `plan:3`），保存与跳过都原样交回去。
//
// 单独成文件的原因与 `inspire-panel.ts` 同一条：这一整块是"会发生什么"，
// `.vue` 那一份是"长什么样"，组件那一层于是**一个 API 都不直接调**。
//
// 三条分寸：
// - **草稿按对留着**：作者在某一句上写了一半又去跳别的对，回来时那一句还在
//   （草稿只活在这一次会话里，不落盘——它还不是"作者的稿子"）；
// - **划过的不再冒头**：跳过落盘（下一次挑对还认），而且当场能撤销；
// - **这一屏是按需读的**：组件挂上、或切了章才读一次，不打开它的人不为它付一次读库。

import { computed, ref, type Ref } from "vue";

import { asError } from "../api/errors.ts";
import {
  inspireCollide,
  inspireCollisions,
  inspireSkipCollision,
  inspireUnskipCollision,
  type Collision,
} from "../api/inspire.ts";
import { t } from "../locales/index.ts";
import { collidePrompt, pairKey } from "./collide.ts";

/** 这一片要的那点外部东西：哪本书、现在哪一章（排序与"撞哪一类"都看它）。 */
export interface CollidePanelOptions {
  workId: Ref<number | null>;
  /** 当前这一章：挑对时跟它有关的排前面 */
  currentChapter: Ref<number | null>;
}

export interface CollidePanelState {
  board: Ref<Collision[]>;
  busy: Ref<boolean>;
  /** 失败时那句**已经渲染好的**话（`CoreError` 走字典渲染，界面不拼中文也不露码） */
  errorText: Ref<string>;
  /** 刚办成的那件事（一句话回执） */
  receipt: Ref<string>;
  /** 刚跳过的那一对（撤销要认它） */
  lastSkipped: Ref<string | null>;
  currentChapter: Ref<number | null>;
  refresh: () => Promise<void>;
  /** 这一对现在的草稿（还没写过就是机制给的那句引子） */
  draftFor: (pair: Collision) => string;
  setDraft: (pair: Collision, text: string) => void;
  /** 存下这一句（存完这一对不再出现） */
  save: (pair: Collision) => Promise<void>;
  skip: (pair: Collision) => Promise<void>;
  undoSkip: () => Promise<void>;
}

export function useCollidePanel(deps: CollidePanelOptions): CollidePanelState {
  const board = ref<Collision[]>([]);
  const busy = ref(false);
  const errorText = ref("");
  const receipt = ref("");
  const lastSkipped = ref<string | null>(null);
  /** 作者在每一对上写的那句（键＝规范写法）；没写过的到时候填机制给的引子 */
  const drafts = ref<Record<string, string>>({});

  const currentChapter = computed(() => deps.currentChapter.value);

  function report(error: unknown) {
    errorText.value = asError(error).message;
  }

  /** 一屏回来了：给**新出现的**那几对填上引子（写过的不动——那是作者的字）。 */
  function fillDrafts(pairs: Collision[]) {
    for (const pair of pairs) {
      const key = pairKey(pair);
      if (drafts.value[key] === undefined) {
        drafts.value = { ...drafts.value, [key]: collidePrompt(pair) };
      }
    }
  }

  async function refresh() {
    const work = deps.workId.value;
    if (!work) {
      board.value = [];
      return;
    }
    try {
      busy.value = true;
      errorText.value = "";
      const pairs = await inspireCollisions(work, deps.currentChapter.value);
      board.value = pairs;
      fillDrafts(pairs);
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  function draftFor(pair: Collision): string {
    return drafts.value[pairKey(pair)] ?? collidePrompt(pair);
  }

  function setDraft(pair: Collision, text: string) {
    drafts.value = { ...drafts.value, [pairKey(pair)]: text };
  }

  /** 存下这一句：写成的是一张普通灵感卡（双源溯源在核心那边刻好）。 */
  async function save(pair: Collision) {
    const body = draftFor(pair).trim();
    if (!body) {
      errorText.value = t("collide.need_words");
      return;
    }
    try {
      busy.value = true;
      errorText.value = "";
      receipt.value = "";
      const done = await inspireCollide(pair.left.key, pair.right.key, body, deps.currentChapter.value);
      board.value = done.collisions;
      fillDrafts(done.collisions);
      lastSkipped.value = null;
      receipt.value = t("collide.saved", { body });
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  /** 跳过这一对（落盘：下一次挑对还认它）。 */
  async function skip(pair: Collision) {
    const work = deps.workId.value;
    if (!work) return;
    try {
      busy.value = true;
      errorText.value = "";
      receipt.value = "";
      board.value = await inspireSkipCollision(
        work,
        pair.left.key,
        pair.right.key,
        deps.currentChapter.value,
      );
      lastSkipped.value = pairKey(pair);
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  /** 撤销刚才那一次跳过（跳错了要能捡回来）。 */
  async function undoSkip() {
    const work = deps.workId.value;
    const key = lastSkipped.value;
    if (!work || key === null) return;
    const [left, right] = key.split("|");
    try {
      busy.value = true;
      errorText.value = "";
      board.value = await inspireUnskipCollision(work, left, right, deps.currentChapter.value);
      fillDrafts(board.value);
      lastSkipped.value = null;
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
    lastSkipped,
    currentChapter,
    refresh,
    draftFor,
    setDraft,
    save,
    skip,
    undoSkip,
  };
}
