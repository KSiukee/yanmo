// 设定卡面板的**状态与命令编排**：列表、新建/修改那张表单、删掉。
//
// 单独成文件的原因与 `creator-panel.ts` 同一条：这一整块是"会发生什么"，
// 而 `.vue` 那一份是"长什么样"；组件那一层于是**一个 API 都不直接调**。
//
// 两条分寸（与核心一致）：
// - **整卡覆盖**：表单是什么样交上去就是什么样；
// - **名字空着不让交**（核心也会拒，但界面先挡住更省事——这不是"前端校验代替后端"，
//   后端那道闸照样在）。

import { computed, onMounted, ref, watch, type Ref } from "vue";

import { asError } from "../api/errors.ts";
import {
  entityCreate,
  entityDelete,
  entityList,
  entityUpdate,
  type EntityBoard,
  type EntityCard,
} from "../api/entity.ts";
import { t } from "../locales/index.ts";
import {
  blankDraft,
  draftOf,
  draftReady,
  filterOptions,
  formOf,
  visibleCards,
  type EntityDraft,
  type EntityFilterOption,
} from "./entity.ts";

export interface EntityPanelOptions {
  /** 当前作品（换书＝换一份名单）；**用 ref**：换书时这一屏要跟着改口径 */
  workId: Ref<number | null>;
}

/** 这一屏给界面用的那一份（组件只读这些，一个 API 都不直接调）。 */
export interface EntityPanelState {
  /** 弹窗开着没有 */
  visible: Ref<boolean>;
  board: Ref<EntityBoard | null>;
  busy: Ref<boolean>;
  /** 失败时那句已经渲染好的话（`CoreError` 走字典渲染） */
  errorText: Ref<string>;
  /** 眼下看哪一项（`all` = 全部） */
  filter: Ref<"all" | "person" | "setting">;
  /** 正在编的那张（`null` = 没开表单） */
  draft: Ref<EntityDraft | null>;
  justSaved: Ref<string>;
  options: Ref<EntityFilterOption[]>;
  list: Ref<EntityCard[]>;
  canSave: Ref<boolean>;
  /** 读一遍这本书的设定卡 */
  load: () => Promise<void>;
  /** 打开 / 收起弹窗 */
  show: () => void;
  hide: () => void;
  /** 开一张新表单 / 改这一张 / 收起表单 */
  startNew: () => void;
  startEdit: (card: EntityCard) => void;
  cancelEdit: () => void;
  /** 存下这张（新建或整卡覆盖） */
  save: () => Promise<void>;
  /** 删掉一张（软删） */
  remove: (card: EntityCard) => Promise<void>;
}

export function useEntityPanel(deps: EntityPanelOptions): EntityPanelState {
  const board = ref<EntityBoard | null>(null);
  const busy = ref(false);
  /** 失败时那句**已经渲染好的**话（`CoreError` 走字典渲染，界面不拼中文也不露码） */
  const errorText = ref("");
  const filter = ref<"all" | "person" | "setting">("all");
  /** 弹窗开着没有（与书架、回收站那些弹窗同一条口径：可见性是会话状态） */
  const visible = ref(false);
  /** 正在编的那张（`null` = 没开表单） */
  const draft = ref<EntityDraft | null>(null);
  const justSaved = ref("");

  const options = computed(() => filterOptions(board.value));
  const list = computed(() => visibleCards(board.value?.cards ?? [], filter.value));
  const canSave = computed(() => draft.value !== null && draftReady(draft.value));

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
      board.value = await entityList(work);
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  /** 打开弹窗（顺手重读一次列表：别拿上次的旧账给作者看）。 */
  function show() {
    visible.value = true;
    justSaved.value = "";
    void load();
  }

  /** 收起弹窗（表单也收起——下次进来从列表开始）。 */
  function hide() {
    visible.value = false;
    draft.value = null;
  }

  /** 开一张新表单（默认记成人物——最常记的就是人）。 */
  function startNew() {
    justSaved.value = "";
    draft.value = blankDraft();
  }

  /** 改这一张（摊成表单）。 */
  function startEdit(card: EntityCard) {
    justSaved.value = "";
    draft.value = draftOf(card);
  }

  /** 表单收起来（不存）。 */
  function cancelEdit() {
    draft.value = null;
  }

  /** 存下这张（新建或整卡覆盖），回执由核心读完再给。 */
  async function save() {
    const work = deps.workId.value;
    const current = draft.value;
    if (!work || !current || !draftReady(current)) return;
    try {
      busy.value = true;
      errorText.value = "";
      const form = formOf(current);
      board.value =
        current.id === null ? await entityCreate(work, form) : await entityUpdate(current.id, form);
      justSaved.value = t("entity.saved", { name: form.name });
      draft.value = null;
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  /** 删掉一张（软删：库里还留着，只是不列出来了）。 */
  async function remove(card: EntityCard) {
    try {
      busy.value = true;
      errorText.value = "";
      board.value = await entityDelete(card.id);
      justSaved.value = t("entity.deleted", { name: card.name });
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  onMounted(() => {
    // 弹窗没开就不用读（它是"打开才看"的一屏，不是常驻画面）
    if (visible.value) void load();
  });
  watch(
    () => deps.workId.value,
    () => {
      // 换书：弹窗收起、表单收起、筛选项与回执都不该跟着新书走（它们说的是上一本的事）
      visible.value = false;
      draft.value = null;
      justSaved.value = "";
      filter.value = "all";
    },
  );

  return {
    visible,
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
    show,
    hide,
    startNew,
    startEdit,
    cancelEdit,
    save,
    remove,
  };
}
