// 设定卡面板的**状态与命令编排**：列表、新建/修改那张表单、删掉。
//
// 单独成文件的原因与 `creator-panel.ts` 同一条：这一整块是"会发生什么"，
// 而 `.vue` 那一份是"长什么样"；组件那一层于是**一个 API 都不直接调**。
//
// 两条分寸（与核心一致）：
// - **整卡覆盖**：表单是什么样交上去就是什么样；
// - **名字空着不让交**（核心也会拒，但界面先挡住更省事——这不是"前端校验代替后端"，
//   后端那道闸照样在）。

import { computed, ref, watch, type Ref } from "vue";

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
import type { EntityKind } from "../api/entity.ts";
import {
  blankDraft,
  draftOf,
  draftReady,
  formOf,
  type EntityDraft,
} from "./entity.ts";

export interface EntityPanelOptions {
  /** 当前作品（换书＝换一份名单）；**用 ref**：换书时这一屏要跟着改口径 */
  workId: Ref<number | null>;
}

/** 这一屏给界面用的那一份（组件只读这些，一个 API 都不直接调）。 */
export interface EntityPanelState {
  board: Ref<EntityBoard | null>;
  busy: Ref<boolean>;
  /** 失败时那句已经渲染好的话（`CoreError` 走字典渲染） */
  errorText: Ref<string>;
  /** 正在编的那张（`null` = 没开表单） */
  draft: Ref<EntityDraft | null>;
  justSaved: Ref<string>;
  /** 这本书全部设定卡（各页自己按类型挑——页签就是筛子） */
  list: Ref<EntityCard[]>;
  canSave: Ref<boolean>;
  /** 读一遍这本书的设定卡 */
  load: () => Promise<void>;
  /** 开一张新表单（给哪一档就记成哪一档）/ 改这一张 / 收起表单 */
  startNew: (kind?: EntityKind) => void;
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
  /** 正在编的那张（`null` = 没开表单） */
  const draft = ref<EntityDraft | null>(null);
  const justSaved = ref("");

  // 名单一次给全：哪一页摆哪一档由页面自己挑（页签就是筛子）
  const list = computed(() => board.value?.cards ?? []);
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

  /** 开一张新表单：**记成哪一档由调用方给**（大纲面板上人物与设定是两页）。 */
  function startNew(kind: EntityKind = "person") {
    justSaved.value = "";
    draft.value = blankDraft(kind);
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

  // 读的时机由「设定」那个弹窗说了算（它是"打开才看"的一屏）：见 lore.ts 的 show()
  watch(
    () => deps.workId.value,
    () => {
      // 换书：表单收起、筛选项与回执都不该跟着新书走（它们说的是上一本的事）
      draft.value = null;
      justSaved.value = "";
    },
  );

  return {
    board,
    busy,
    errorText,
    draft,
    justSaved,
    list,
    canSave,
    load,
    startNew,
    startEdit,
    cancelEdit,
    save,
    remove,
  };
}
