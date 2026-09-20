// 流程那一屏的**状态与命令编排**：读哪一层、改一步、存回去。
//
// 单独成文件的原因与 `creator-panel.ts` 同一条：这一整块是"会发生什么"，
// `.vue` 那一份是"长什么样"，组件那一层于是**一个 API 都不直接调**。
//
// 三条分寸：
// - **改一下就存一下**：没有"保存"按钮。文本类（名字 / 说明 / 时长 / 检查项文字）在**失焦**
//   时才写（一处改动一条留痕）；结构类（加步 / 挪位 / 跳过 / 删 / 勾）当场写；
// - **写完回读**：界面显示的永远是库里那份（校验夹过、空清单当删掉都在核心做了）；
// - **作用范围是"整层"**：全局那份是"我平时的写法"，这一本可以单独一套——
//   两层之间是**不覆盖 / 覆盖**，不做字段级合并（一条流程是一份有序清单）。

import { computed, ref, type Ref } from "vue";

import { asError } from "../api/errors.ts";
import {
  sopMechBook,
  sopMechChapter,
  sopRead,
  sopWrite,
  sopReset,
  type BookMechFacts,
  type ChapterMechFacts,
  type ResolvedSop,
  type SopCheck,
  type SopMech,
  type SopStep,
} from "../api/sop.ts";
import { t } from "../locales/index.ts";
import { cloneSteps, mechReading, nextId, summary, toSop, type MechReading } from "./sop.ts";

/** 这一片要的那点外部东西：现在开着哪本书、哪一章（机械项的两种视角各看一个）。 */
export interface SopPanelOptions {
  workId: Ref<number | null>;
  /** 当前这一章（没开章就是 `null`）：机械项"本章"那一档的读数看它 */
  nodeId: Ref<number | null>;
}

export interface SopPanelState {
  board: Ref<ResolvedSop | null>;
  busy: Ref<boolean>;
  /** 失败时那句**已经渲染好的**话（`CoreError` 走字典渲染，界面不拼中文也不露码） */
  errorText: Ref<string>;
  /** 刚办成的那件事（一句话回执） */
  receipt: Ref<string>;
  /** 现在改的是哪一层：`true` = 这一本单独一套，`false` = 我平时的写法 */
  workScope: Ref<boolean>;
  /** 这一层的步骤（改一下就存一下，所以它同时是"输入"与"库里的那份"） */
  list: Ref<SopStep[]>;
  /** 一屏读数（几步 / 跳过几步 / 勾了几项） */
  summaryText: Ref<string>;
  /** 这一本单独一套那一档能不能选（没开书就不能） */
  canUseWorkScope: Ref<boolean>;
  /** 机械项"本章"的读数（事实；没取到就是 `null`——界面说"读数还没到"） */
  chapterFacts: Ref<ChapterMechFacts | null>;
  /** 机械项"全书"的读数（要扫全稿，所以只在真需要时取） */
  bookFacts: Ref<BookMechFacts | null>;
  /** 机械项正在算（那一下可能有点慢——全书那一档要读每一章正文） */
  mechBusy: Ref<boolean>;
  /** 一条检查项现在的读数；`null` = 人工那条（作者自己勾） */
  readingOf: (check: SopCheck) => MechReading | null;
  /** 按需要一次读数（换章 / 换书 / 换层之后调；单子没变就不重算） */
  syncFacts: () => Promise<void>;
  /** 重算机械项的读数（正文改过之后点一下） */
  recheck: () => Promise<void>;
  refresh: () => Promise<void>;
  setScope: (work: boolean) => Promise<void>;
  /** 加一步（名字留空，作者在名字框里写——**不是**"下一步"那种强制顺序） */
  addStep: () => Promise<void>;
  moveStep: (id: string, delta: number) => Promise<void>;
  removeStep: (id: string) => Promise<void>;
  toggleSkip: (id: string) => Promise<void>;
  renameStep: (id: string, name: string) => Promise<void>;
  setNote: (id: string, note: string) => Promise<void>;
  setMinutes: (id: string, text: string) => Promise<void>;
  addCheck: (stepId: string) => Promise<void>;
  /** 加一条**机械项**（判据 + 视角都得在核心的白名单里，认不出会当场报错） */
  addMech: (stepId: string, scope: string, judge: string) => Promise<void>;
  setCheckText: (stepId: string, checkId: string, text: string) => Promise<void>;
  toggleCheck: (stepId: string, checkId: string) => Promise<void>;
  removeCheck: (stepId: string, checkId: string) => Promise<void>;
  /** 绑一个动作（`""` = 不绑）；只认"打开哪块面板" */
  setAction: (stepId: string, value: string) => Promise<void>;
  /** 清掉这一层（书的覆盖则是"回到跟全局一份"） */
  reset: () => Promise<void>;
}

export function useSopPanel(deps: SopPanelOptions): SopPanelState {
  const board = ref<ResolvedSop | null>(null);
  const busy = ref(false);
  const errorText = ref("");
  const receipt = ref("");
  /** 现在改哪一层（默认"我平时的写法"——SOP **跟人**是常态，按作品覆盖是例外） */
  const workScope = ref(false);

  const canUseWorkScope = computed(() => deps.workId.value !== null);
  const list = computed(() => board.value?.steps ?? []);
  const summaryText = computed(() => summary(list.value));

  function report(error: unknown) {
    errorText.value = asError(error).message;
  }

  /** 这一层的 `work_id`（`null` = 全局那份）。 */
  function layer(): number | null {
    return workScope.value && deps.workId.value !== null ? deps.workId.value : null;
  }

  // ── 机械项的读数：**按需取** ──────────────────────────────────────────
  //
  // 这一份流程里没有哪一种视角，就不去读那一种——"全书"那一档要读每一章正文
  //（核心复用「计划 vs 实际」那次全书扫描），白扫一遍是拿作者的时间开玩笑。
  const chapterFacts = ref<ChapterMechFacts | null>(null);
  const bookFacts = ref<BookMechFacts | null>(null);
  const mechBusy = ref(false);
  /** 上一次算的是哪一张单子（章节 / 书 / 要哪几种读数）：没变就不重算 */
  let wantedSignature: string | null = null;

  /** 这一份流程里的机械项要哪几种读数（去重排序：同一张单子换个顺序不算变） */
  function wanted(steps: SopStep[]): string[] {
    const keys = steps
      .flatMap((step) => step.checks)
      .map((check) => check.mech)
      .filter((mech): mech is SopMech => mech !== null)
      .map((mech) => `${mech.scope}:${mech.judge}`);
    return [...new Set(keys)].sort();
  }

  async function refreshFacts(force = false) {
    const keys = wanted(list.value);
    const signature = `${deps.nodeId.value}|${deps.workId.value}|${keys.join(",")}`;
    if (!force && signature === wantedSignature) return;
    wantedSignature = signature;
    try {
      mechBusy.value = true;
      errorText.value = "";
      chapterFacts.value =
        keys.some((key) => key.startsWith("chapter:")) && deps.nodeId.value !== null
          ? await sopMechChapter(deps.nodeId.value)
          : null;
      bookFacts.value =
        keys.some((key) => key.startsWith("book:")) && deps.workId.value !== null
          ? await sopMechBook(deps.workId.value)
          : null;
    } catch (error) {
      report(error);
    } finally {
      mechBusy.value = false;
    }
  }

  /** 一条机械项念成什么——模板里直接调它，读数变了会跟着重画。 */
  function readingOf(check: SopCheck): MechReading | null {
    return mechReading(check, chapterFacts.value, bookFacts.value);
  }

  async function refresh() {
    try {
      busy.value = true;
      errorText.value = "";
      board.value = await sopRead(layer());
      // 换了一层：单子可能整份都不同了（签名里没带层，所以这里要强制重算一次）
      wantedSignature = null;
      await refreshFacts();
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  /**
   * 改一处就存一处：把步骤改完**整份**写回去，拿回读那份当新的显示。
   *
   * `clean` 是这次改动之后要清掉的回执（比如换了一层时不清"存下了"那句）。
   */
  async function commit(change: (steps: SopStep[]) => SopStep[]) {
    const current = board.value;
    if (!current) return;
    try {
      busy.value = true;
      errorText.value = "";
      const steps = change(cloneSteps(current.steps));
      board.value = await sopWrite(layer(), toSop(steps));
      receipt.value = t("sop.saved");
      // 加了 / 删了机械项，要读的东西就变了；只是改个名字的话签名没变，一次请求都不发
      await refreshFacts();
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  /** 换一层：书那一层没设过时读到的是继承来的那份，**一改就有了自己的一套**。 */
  async function setScope(work: boolean) {
    if (work && !canUseWorkScope.value) return;
    workScope.value = work;
    receipt.value = "";
    await refresh();
  }

  return {
    board,
    busy,
    errorText,
    receipt,
    workScope,
    list,
    summaryText,
    canUseWorkScope,
    chapterFacts,
    bookFacts,
    mechBusy,
    readingOf,
    syncFacts: () => refreshFacts(),
    recheck: () => refreshFacts(true),
    refresh,
    setScope,
    async addStep() {
      await commit((steps) => {
        const id = nextId("custom", steps.map((step) => step.id));
        steps.push({
          id,
          name: null,
          note: null,
          minutes: null,
          checks: [],
          action: null,
          skipped: false,
        });
        return steps;
      });
    },
    async moveStep(id, delta) {
      await commit((steps) => {
        const at = steps.findIndex((step) => step.id === id);
        const to = at + delta;
        if (at < 0 || to < 0 || to >= steps.length) return steps;
        const [moved] = steps.splice(at, 1);
        steps.splice(to, 0, moved);
        return steps;
      });
    },
    async removeStep(id) {
      await commit((steps) => steps.filter((step) => step.id !== id));
    },
    async toggleSkip(id) {
      await commit((steps) =>
        steps.map((step) => (step.id === id ? { ...step, skipped: !step.skipped } : step)),
      );
    },
    async renameStep(id, name) {
      const trimmed = name.trim();
      await commit((steps) =>
        steps.map((step) =>
          step.id === id ? { ...step, name: trimmed.length > 0 ? trimmed : null } : step,
        ),
      );
    },
    async setNote(id, note) {
      const trimmed = note.trim();
      await commit((steps) =>
        steps.map((step) =>
          step.id === id ? { ...step, note: trimmed.length > 0 ? trimmed : null } : step,
        ),
      );
    },
    async setMinutes(id, text) {
      const parsed = Number.parseInt(text.trim(), 10);
      const minutes = Number.isFinite(parsed) && parsed > 0 ? parsed : null;
      await commit((steps) =>
        steps.map((step) => (step.id === id ? { ...step, minutes } : step)),
      );
    },
    async addCheck(stepId) {
      await commit((steps) =>
        steps.map((step) => {
          if (step.id !== stepId) return step;
          const id = nextId("check", step.checks.map((check) => check.id));
          return { ...step, checks: [...step.checks, { id, text: null, done: false, mech: null }] };
        }),
      );
    },
    async addMech(stepId, scope, judge) {
      await commit((steps) =>
        steps.map((step) => {
          if (step.id !== stepId) return step;
          const id = nextId("mech", step.checks.map((check) => check.id));
          return {
            ...step,
            checks: [...step.checks, { id, text: null, done: false, mech: { scope, judge } }],
          };
        }),
      );
    },
    async setCheckText(stepId, checkId, text) {
      const trimmed = text.trim();
      await commit((steps) =>
        steps.map((step) =>
          step.id === stepId
            ? {
                ...step,
                checks: step.checks.map((check) =>
                  check.id === checkId
                    ? { ...check, text: trimmed.length > 0 ? trimmed : null }
                    : check,
                ),
              }
            : step,
        ),
      );
    },
    async toggleCheck(stepId, checkId) {
      await commit((steps) =>
        steps.map((step) =>
          step.id === stepId
            ? {
                ...step,
                checks: step.checks.map((check) =>
                  check.id === checkId ? { ...check, done: !check.done } : check,
                ),
              }
            : step,
        ),
      );
    },
    async removeCheck(stepId, checkId) {
      await commit((steps) =>
        steps.map((step) =>
          step.id === stepId
            ? { ...step, checks: step.checks.filter((check) => check.id !== checkId) }
            : step,
        ),
      );
    },
    async setAction(stepId, value) {
      const trimmed = value.trim();
      await commit((steps) =>
        steps.map((step) =>
          step.id === stepId
            ? { ...step, action: trimmed.length > 0 ? { target: "aside", value: trimmed } : null }
            : step,
        ),
      );
    },
    async reset() {
      try {
        busy.value = true;
        errorText.value = "";
        board.value = await sopReset(layer());
        receipt.value = t("sop.reset_done");
      } catch (error) {
        report(error);
      } finally {
        busy.value = false;
      }
    },
  };
}
