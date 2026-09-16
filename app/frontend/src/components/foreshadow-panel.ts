// 伏笔那一屏的**状态与命令编排**：记一条、改一条、走一步状态、删掉。
//
// 单独成文件的原因与 `entity-panel.ts` 同一条：这一整块是"会发生什么"，
// 而 `.vue` 那一份是"长什么样"。
//
// 三条分寸（与核心一致）：
// - **弹窗的可见性与页签归 [`useLore`](./lore.ts)**（这里只管这一屏自己的事）；
// - **走一步要合法**：按钮由 `nextStates` 摆，核心再挡一道；
// - **"不写了"是正经结局**——文案不许写成"未完成"（语气层的一贯立场）。

import { computed, ref, watch, type Ref } from "vue";

import { asError } from "../api/errors.ts";
import {
  foreshadowCreate,
  foreshadowDelete,
  foreshadowList,
  foreshadowMove,
  foreshadowUpdate,
  type Foreshadow,
  type ForeshadowBoard,
  type ForeshadowState,
} from "../api/foreshadow.ts";
import { t } from "../locales/index.ts";
import { filterOptions, stateLabel, visibleItems } from "./foreshadow.ts";

export interface ForeshadowPanelOptions {
  /** 当前作品（换书＝换一份账） */
  workId: Ref<number | null>;
  /** 记一条时默认埋在**当前这一章**（开着某一章才给） */
  currentChapter: Ref<number | null>;
}

/** 表单里那一份（新建与改共用；`id` 为 `null` 就是新建）。 */
export interface ForeshadowDraft {
  id: number | null;
  body: string;
  planted_node: number | null;
  note: string;
}

/** 这一屏给界面用的那一份（组件只读这些，一个 API 都不直接调）。 */
export interface ForeshadowPanelState {
  board: Ref<ForeshadowBoard | null>;
  busy: Ref<boolean>;
  /** 失败时那句已经渲染好的话（`CoreError` 走字典渲染） */
  errorText: Ref<string>;
  filter: Ref<ForeshadowState | "all">;
  draft: Ref<ForeshadowDraft | null>;
  justSaved: Ref<string>;
  options: Ref<ReturnType<typeof filterOptions>>;
  list: Ref<Foreshadow[]>;
  canSave: Ref<boolean>;
  load: () => Promise<void>;
  /** 开一张新表单（默认埋在**当前这一章**） */
  startNew: () => void;
  startEdit: (item: Foreshadow) => void;
  cancelEdit: () => void;
  save: () => Promise<void>;
  /** 走一步状态；收了的时候默认记"收在当前这一章" */
  move: (item: Foreshadow, to: ForeshadowState) => Promise<void>;
  remove: (item: Foreshadow) => Promise<void>;
}

export function useForeshadowPanel(deps: ForeshadowPanelOptions): ForeshadowPanelState {
  const board = ref<ForeshadowBoard | null>(null);
  const busy = ref(false);
  const errorText = ref("");
  const filter = ref<ForeshadowState | "all">("all");
  const draft = ref<ForeshadowDraft | null>(null);
  const justSaved = ref("");

  const options = computed(() => filterOptions(board.value));
  const list = computed(() => visibleItems(board.value?.items ?? [], filter.value));
  const canSave = computed(() => draft.value !== null && draft.value.body.trim() !== "");

  function report(error: unknown) {
    errorText.value = asError(error).message;
  }

  async function load() {
    const work = deps.workId.value;
    if (!work) {
      board.value = null;
      return;
    }
    try {
      busy.value = true;
      errorText.value = "";
      board.value = await foreshadowList(work);
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  /** 开一张新表单：**默认埋在手里这一章**（作者多半是在写到那儿时想起来的）。 */
  function startNew() {
    justSaved.value = "";
    draft.value = {
      id: null,
      body: "",
      planted_node: deps.currentChapter.value,
      note: "",
    };
  }

  function startEdit(item: Foreshadow) {
    justSaved.value = "";
    draft.value = {
      id: item.id,
      body: item.body,
      planted_node: item.planted_node,
      note: item.note,
    };
  }

  function cancelEdit() {
    draft.value = null;
  }

  async function save() {
    const work = deps.workId.value;
    const current = draft.value;
    if (!work || !current || !canSave.value) return;
    try {
      busy.value = true;
      errorText.value = "";
      board.value =
        current.id === null
          ? await foreshadowCreate(work, current.body, current.planted_node, current.note)
          : await foreshadowUpdate(current.id, current.body, current.planted_node, current.note);
      justSaved.value = t("foreshadow.saved", { body: current.body.trim() });
      draft.value = null;
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  async function move(item: Foreshadow, to: ForeshadowState) {
    try {
      busy.value = true;
      errorText.value = "";
      // 收了：默认把"收在哪一章"记成**手里这一章**（作者多半就站在收线的那一章上）
      const collected = to === "collected" ? deps.currentChapter.value : null;
      board.value = await foreshadowMove(item.id, to, collected);
      justSaved.value = t("foreshadow.moved", { body: item.body, state: stateLabel(to) });
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  async function remove(item: Foreshadow) {
    try {
      busy.value = true;
      errorText.value = "";
      board.value = await foreshadowDelete(item.id);
      justSaved.value = "";
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  // 读的时机由「设定」那个弹窗说了算（它是"打开才看"的一屏）：见 lore.ts 的 show()
  watch(
    () => deps.workId.value,
    () => {
      // 换书：表单收起、筛选项与回执都不该跟着新书走
      draft.value = null;
      justSaved.value = "";
      filter.value = "all";
      board.value = null;
    },
  );

  return {
    board,
    busy,
    errorText,
    filter,
    draft,
    justSaved,
    options,
    list,
    canSave,
    load,
    startNew,
    startEdit,
    cancelEdit,
    save,
    move,
    remove,
  };
}
