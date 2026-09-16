// 创作流面板的**状态与命令编排**：记一条、读回来、删掉、撤销。
//
// 单独成文件的原因与 `flow-panel.ts` 同一条：这一整块是"会发生什么"，
// 而 `.vue` 那一份是"长什么样"；组件那一层于是**一个 API 都不直接调**。
//
// 两条分寸（与核心一致）：
// - **记下来不打断**：写一条碎片没有别的副作用（不动正文、不动叩问的问题状态）；
// - **删是软删**：所以"撤销"当场就能兑现（捞回就是把时间戳抹掉）。

import { computed, onMounted, ref, watch, type Ref } from "vue";

import { asError } from "../api/errors.ts";
import {
  fragmentAdd,
  fragmentBoard,
  fragmentDelete,
  fragmentRestore,
  fragmentUpdate,
  type CreatorBoard,
  type Fragment,
  type FragmentKind,
} from "../api/fragment.ts";
import { t } from "../locales/index.ts";
import {
  canCarryStoryTime,
  filterOptions,
  kindLabel,
  parseStoryOrder,
  visibleFragments,
} from "./creator.ts";

/** 面板要的那点外部东西：哪本书，以及"当前这一章"（记一条时做关联锚点）。 */
export interface CreatorPanelOptions {
  workId: Ref<number | null>;
  /** 当前这一章（没有就是空）：记一条时把"是在这一章写的"记成锚点 */
  currentChapter: Ref<number | null>;
}

/** 这一屏给界面用的那一份（创作流那一栏与「大纲」的事件页共用它）。 */
export interface CreatorPanelState {
  board: Ref<CreatorBoard | null>;
  busy: Ref<boolean>;
  errorText: Ref<string>;
  filter: Ref<FragmentKind | "all">;
  draftKind: Ref<FragmentKind>;
  justSaved: Ref<string>;
  lastDeleted: Ref<number | null>;
  editing: Ref<FragmentDraft | null>;
  options: Ref<{ kind: FragmentKind | "all"; count: number }[]>;
  list: Ref<Fragment[]>;
  canJot: Ref<boolean>;
  refresh: () => Promise<void>;
  jot: (body: string, kind?: FragmentKind) => Promise<boolean>;
  startEdit: (item: Fragment) => void;
  cancelEdit: () => void;
  saveEdit: () => Promise<boolean>;
  remove: (id: number) => Promise<void>;
  undo: () => Promise<void>;
}

/** 改一条事件时手上那一份（**数字那一栏是文本框里的原文**，存的时候才解析）。 */
export interface FragmentDraft {
  id: number;
  /** 正文（一起交回去：接口要一次给全） */
  body: string;
  /** 原样摆回文本框的排序值（空串 = 没填） */
  storyOrderText: string;
  /** 自由文本的故事时间（承平三年·春） */
  storyTime: string;
  flashback: boolean;
}

export function useCreatorPanel(deps: CreatorPanelOptions): CreatorPanelState {
  const board = ref<CreatorBoard | null>(null);
  const busy = ref(false);
  /** 失败时那句**已经渲染好的**话（`CoreError` 走字典渲染，界面不拼中文也不露码）。 */
  const errorText = ref("");
  /** 眼下看哪一档（**不落盘**：它是"当下这一会儿想看什么"，不是长期偏好） */
  const filter = ref<FragmentKind | "all">("all");
  /** "记成"里选的那一种（默认灵感——它是最常记的） */
  const draftKind = ref<FragmentKind>("idea");
  const justSaved = ref("");
  /** 刚删掉的那一条：撤销要认它（只留最近一条，够用） */
  const lastDeleted = ref<number | null>(null);
  /** 正在改的那一条（事件的正文 + 故事时间三样；`null` = 没在改） */
  const editing = ref<FragmentDraft | null>(null);

  /** 当前这一章：记一条时把"是在这一章写的"记成锚点 */
  const currentChapter = computed(() => deps.currentChapter.value);
  const options = computed(() => filterOptions(board.value));
  const list = computed(() => visibleFragments(board.value?.fragments ?? [], filter.value));
  const canJot = computed(() => deps.workId.value !== null);

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
      board.value = await fragmentBoard(work);
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  /**
   * 记一条。
   *
   * 锚点由界面给（核心只存不懂）：开着某一章就记成 `chapter:<章 id>`，
   * 于是"这条是写这一章时想起来的"永远查得到（将来按章找回来也认它）。
   */
  async function jot(body: string, kind?: FragmentKind): Promise<boolean> {
    const work = deps.workId.value;
    if (!work || !body.trim()) return false;
    const node = currentChapter.value;
    const anchors = node === null ? [] : [`chapter:${node}`];
    try {
      busy.value = true;
      errorText.value = "";
      // 不带 kind 就用"记成"里选的那一档；带了就用它（大纲面板那一页只用事件）
      const saved = await fragmentAdd(work, kind ?? draftKind.value, body, "typed", anchors);
      justSaved.value = t("creator.jot.done", {
        kind: kindLabel(saved.kind),
        body: saved.body,
      });
      lastDeleted.value = null;
      // 记下的那条一定在"全部"里；作者正筛着别的档时切回全部，免得他以为没记上
      filter.value = "all";
      board.value = await fragmentBoard(work);
      return true;
    } catch (error) {
      report(error);
      return false;
    } finally {
      busy.value = false;
    }
  }

  /** 开一条事件来改（只有事件有故事时间那一栏）。 */
  function startEdit(item: Fragment) {
    if (!canCarryStoryTime(item.kind)) return;
    justSaved.value = "";
    editing.value = {
      id: item.id,
      body: item.body,
      storyOrderText: item.story_order === null ? "" : String(item.story_order),
      storyTime: item.story_time,
      flashback: item.flashback,
    };
  }

  /** 收起编辑（不存）。 */
  function cancelEdit() {
    editing.value = null;
  }

  /** 存下这一条（正文 + 故事时间一次给全）。 */
  async function saveEdit() {
    const work = deps.workId.value;
    const current = editing.value;
    if (!work || !current || !current.body.trim()) return false;
    try {
      busy.value = true;
      errorText.value = "";
      board.value = await fragmentUpdate(
        current.id,
        current.body,
        current.storyTime,
        parseStoryOrder(current.storyOrderText),
        current.flashback,
      );
      justSaved.value = t("creator.saved", { body: current.body.trim() });
      editing.value = null;
      return true;
    } catch (error) {
      report(error);
      return false;
    } finally {
      busy.value = false;
    }
  }

  /** 删掉一条（软删）：撤销要认它，所以把 id 留着。 */
  async function remove(id: number) {
    try {
      busy.value = true;
      errorText.value = "";
      board.value = await fragmentDelete(id);
      lastDeleted.value = id;
      justSaved.value = "";
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  /** 撤销刚才那一次删除（重复点不算错——已经在的不再动它）。 */
  async function undo() {
    const id = lastDeleted.value;
    if (id === null) return;
    try {
      busy.value = true;
      errorText.value = "";
      board.value = await fragmentRestore(id);
      lastDeleted.value = null;
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  onMounted(() => void refresh());
  watch(() => deps.workId.value, () => {
    // 换书：筛选项、回执与编辑框都不该跟着新书走（它们说的是上一本的事）
    filter.value = "all";
    justSaved.value = "";
    lastDeleted.value = null;
    editing.value = null;
    void refresh();
  });

  return {
    board,
    busy,
    errorText,
    filter,
    draftKind,
    justSaved,
    lastDeleted,
    editing,
    options,
    list,
    canJot,
    refresh,
    jot,
    startEdit,
    cancelEdit,
    saveEdit,
    remove,
    undo,
  };
}
