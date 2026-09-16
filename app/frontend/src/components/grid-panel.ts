// 大纲表的**状态与命令编排**：读全树、逐格存、整片粘、拖动换位、选人、切列、折叠、只看没填的。
//
// 单独成文件的原因与别的 `*-panel.ts` 同一条：这一整块是"会发生什么"，
// `.vue` 那一份只管"长什么样"。
//
// 五条分寸：
// 1. **逐格存**：改哪一格只发那一格（一句话走 `set_node_summary`，四格一次发四格），
//    存下来的回执是**库里真有的那一份**——界面显示的永远是库里的值，不是自己回显的；
// 2. **一片粘一次**：整片粘贴走 `save_outline_cells`（一片一次事务，要么全落要么不落）；
// 3. **打开才读**：它是"看一眼/改一遍"的一屏，不是常驻画面；
// 4. **列与折叠是"当下想看什么"**：不落盘（与第二栏那些开关同一条规矩）；
// 5. **不猜**：存不下就那一格报错、保持原值（绝不假装存上了）。

import { computed, ref, watch, type ComputedRef, type Ref } from "vue";

import { asError } from "../api/errors.ts";
import { outlinePasteCells, outlineRows, type OutlineRowDto } from "../api/outline.ts";
import { setNodeSummary, saveNodeFields, treeMoveNode } from "../api/core.ts";
import { t } from "../locales/index.ts";
import { dropPlan, type DropZone } from "./drop-plan.ts";
import { useGridCast } from "./grid-cast.ts";
import { parsePasteTable, pasteProblemText, planPaste } from "./grid-paste.ts";
import { COLUMNS, DEFAULT_COLUMNS, visibleRows, type ColumnSpec, type GridColumn } from "./grid.ts";

export interface GridPanelOptions {
  /** 当前作品（换书＝换一张表） */
  workId: Ref<number | null>;
  /** 跳到某一章（点章名那一格） */
  openNode: (node_id: number) => Promise<void>;
  /** 在某个分组下面新建一章（表尾那个「+」）；返回新节点的 id */
  createChapter: (parent_id: number | null) => Promise<number | null>;
}

export interface GridPanelState {
  /** 这一屏开着没有（它是**整块主区**上的一层，不是小弹窗） */
  visible: Ref<boolean>;
  rows: Ref<OutlineRowDto[]>;
  /** 表里此刻真摆着的行（折叠与筛选之后）——粘贴按它算落点 */
  shown: ComputedRef<OutlineRowDto[]>;
  /** 整张表的行（`{id, parent_id}`，拖动落点用；拖动中每帧都要读，所以先算好） */
  dropRows: ComputedRef<{ id: number; parent_id: number | null }[]>;
  busy: Ref<boolean>;
  /** 失败时那句已经渲染好的话（`CoreError` 走字典渲染） */
  errorText: Ref<string>;
  /** 现在露哪几列（勾选开关；不落盘） */
  columns: Ref<GridColumn[]>;
  /** 现在露着的那几列（顺序就是表里的顺序） */
  shownColumns: ComputedRef<ColumnSpec[]>;
  /** 只看"要管一下"的行（没写一句话、或四格填了一半） */
  onlyUnfilled: Ref<boolean>;
  /** 收起来的卷 */
  collapsed: Ref<Set<number>>;
  /** 刚存过的那一格 / 刚粘完那一片（界面上闪一句**已经渲染好的**回执） */
  justSaved: Ref<string>;
  /** 现在就开着选人卡的那一行（没有就是 null） */
  castFor: Ref<number | null>;
  /** 这本书的人物卡（选人卡打开时读的） */
  castCards: Ref<import("../api/entity.ts").EntityCard[]>;
  show: () => void;
  hide: () => void;
  toggle: () => void;
  load: () => Promise<void>;
  /** 切一列的显示 */
  toggleColumn: (column: GridColumn) => void;
  /** 收起 / 摊开一卷 */
  toggleCollapsed: (node_id: number) => void;
  /** 存这一章的一句话 */
  saveSummary: (node_id: number, text: string) => Promise<void>;
  /** 存这一行四格里的某一格（**整行四格一起发**：接口要一次给全） */
  saveField: (node_id: number, field: "pov" | "goal" | "conflict" | "outcome", value: string) => Promise<void>;
  /** 凭空降下来的那一片（锚点是作者点下去的那一格） */
  paste: (anchor: { row: number; column: GridColumn }, text: string) => Promise<void>;
  /** 拖动一行到某一行上（落点由纯逻辑算，见 `drop-plan.ts`） */
  drop: (dragged_id: number, target_id: number, zone: DropZone) => Promise<void>;
  /** 打开某一行的选人卡 */
  openCast: (node_id: number) => Promise<void>;
  /** 收起选人卡 */
  closeCast: () => void;
  /** 选 / 不选一个人 */
  toggleCast: (node_id: number, entity_id: number) => Promise<void>;
  /** 跳到这一章（顺手把表收起来） */
  open: (node_id: number) => Promise<void>;
  /** 在这个分组下面加一章 */
  addChapter: (parent_id: number | null) => Promise<void>;
}

export function useOutlineGrid(deps: GridPanelOptions): GridPanelState {
  const visible = ref(false);
  const rows = ref<OutlineRowDto[]>([]);
  const busy = ref(false);
  const errorText = ref("");
  const columns = ref<GridColumn[]>([...DEFAULT_COLUMNS]);
  const onlyUnfilled = ref(false);
  const collapsed = ref<Set<number>>(new Set());
  const justSaved = ref("");

  /** 这一行在手上那份里的位置（找不到就是被别处删了——如实当没这一行）。 */
  function rowOf(node_id: number): OutlineRowDto | undefined {
    return rows.value.find((row) => row.node_id === node_id);
  }

  function replace(next: OutlineRowDto) {
    rows.value = rows.value.map((row) => (row.node_id === next.node_id ? next : row));
  }

  function report(error: unknown) {
    errorText.value = asError(error).message;
  }

  const shown = computed(() =>
    visibleRows(rows.value, { collapsed: collapsed.value, onlyUnfilled: onlyUnfilled.value }),
  );
  /** 露出来的列（保持 COLUMNS 的固定顺序）。 */
  const shownColumns = computed(() => COLUMNS.filter((spec) => columns.value.includes(spec.key)));
  /**
   * 拖动落点算法要的那点行信息（`{id, parent_id}`），**先算好**。
   *
   * 放在这儿算一次，而不是每次 `dragover` 现从 `rows` 摊一遍：拖动时那个事件每秒要响几十次，
   * 几百章的书上等于每帧重建一张表（白发热）。
   */
  const dropRows = computed(() =>
    rows.value.map((row) => ({ id: row.node_id, parent_id: row.parent_id })),
  );

  const cast = useGridCast({ workId: deps.workId, replace, rowOf, report, busy });

  async function load() {
    const work = deps.workId.value;
    if (!work) {
      rows.value = [];
      return;
    }
    try {
      busy.value = true;
      errorText.value = "";
      rows.value = await outlineRows(work);
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  async function saveSummary(node_id: number, text: string) {
    const before = rowOf(node_id);
    if (!before) return;
    try {
      busy.value = true;
      errorText.value = "";
      await setNodeSummary(node_id, text);
      // 核心这条命令只回 void：**写上的是作者刚写的字**（它自己会修剪，但摘要不做修剪），
      // 所以本地就按提交值更新；下一次 `load()` 拿库里的真值。
      replace({ ...before, summary: text });
      justSaved.value = t("grid.saved");
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  async function saveField(
    node_id: number,
    field: "pov" | "goal" | "conflict" | "outcome",
    value: string,
  ) {
    const before = rowOf(node_id);
    if (!before) return;
    try {
      busy.value = true;
      errorText.value = "";
      const saved = await saveNodeFields({ ...before.fields, node_id, [field]: value });
      // 回执是**库里真有的那一份**（修剪过的）：拿它换掉手上那一行
      replace({ ...before, fields: saved });
      justSaved.value = t("grid.saved");
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  /** 表里列的顺序是固定的（勾出来的列不该跑到最后去）。 */
  const COLUMN_ORDER: GridColumn[] = [
    "title",
    "summary",
    "cast",
    "pov",
    "goal",
    "conflict",
    "outcome",
    "foreshadow",
    "words",
  ];

  function toggleColumn(column: GridColumn) {
    const next = columns.value.includes(column)
      ? columns.value.filter((item) => item !== column)
      : [...columns.value, column];
    columns.value = COLUMN_ORDER.filter((item) => next.includes(item));
  }

  function toggleCollapsed(node_id: number) {
    const next = new Set(collapsed.value);
    if (next.has(node_id)) next.delete(node_id);
    else next.add(node_id);
    collapsed.value = next;
  }

  async function open(node_id: number) {
    visible.value = false;
    await deps.openNode(node_id);
  }

  /**
   * 整片粘贴：**锚点就是作者点下去的那一格**，往右往下铺。
   *
   * 落点由纯逻辑算（`grid-paste.ts`）：铺不下、铺到了卷或只读列，都在这里当场说清，
   * 一个字都不发出去（核心那边还有一道核对，见 `store::outline_paste`）。
   */
  async function paste(anchor: { row: number; column: GridColumn }, text: string) {
    const work = deps.workId.value;
    if (!work) return;
    const column = shownColumns.value.findIndex((spec) => spec.key === anchor.column);
    if (column < 0) return;
    const plan = planPaste(parsePasteTable(text), shown.value, shownColumns.value.map((s) => s.key), {
      row: anchor.row,
      column,
    });
    if ("problem" in plan) {
      // 粘不下：说清是哪儿不对，**一格都不动**（这一句不是"存失败"，是"没发出去"）
      errorText.value = pasteProblemText(plan.problem);
      return;
    }
    if (plan.cells.length === 0) {
      // 整片都落在非打字的列上：什么都没写，也别说"粘进来了"
      errorText.value = pasteProblemText({ kind: "empty" });
      return;
    }
    try {
      busy.value = true;
      errorText.value = "";
      rows.value = await outlinePasteCells(work, plan.cells);
      const touched = new Set(plan.cells.map((cell) => cell.node_id)).size;
      const done = t("grid.paste.done", { cells: plan.cells.length, rows: touched });
      // 跳过了哪几列要说出来（不然作者以为整张表都进来了）
      const skipped = plan.skipped.length
        ? " " + t("grid.paste.skipped", {
            columns: plan.skipped.map((column) => t(`grid.col.${column}`)).join(t("common.list_separator")),
          })
        : "";
      justSaved.value = done + skipped;
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  /**
   * 拖动一行：落点 → (新父级, 第几位) 由 `drop-plan.ts` 算（目录树与这里共用一份）。
   *
   * 落了之后**重读整张表**：换卷 / 换序会连带改号的渲染，重读才是库里的真样子
   * （只挪手上那一行的话，界面上会留着一份"看起来挪过了"的假象）。
   */
  async function drop(dragged_id: number, target_id: number, zone: DropZone) {
    const landing = dropPlan(dropRows.value, dragged_id, target_id, zone);
    if (landing === null) return;
    try {
      busy.value = true;
      errorText.value = "";
      await treeMoveNode(dragged_id, landing.parent_id, landing.index);
      await load();
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  async function addChapter(parent_id: number | null) {
    try {
      busy.value = true;
      errorText.value = "";
      const created = await deps.createChapter(parent_id);
      if (created !== null) await load();
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  watch(
    () => deps.workId.value,
    () => {
      // 换书：这一屏整个换一份（列与筛选留着——那是"我想怎么看"，跟书无关）
      rows.value = [];
      collapsed.value = new Set();
      justSaved.value = "";
      errorText.value = "";
      cast.reset();
      if (visible.value) void load();
    },
  );

  return {
    visible,
    rows,
    shown,
    dropRows,
    busy,
    errorText,
    columns,
    shownColumns,
    onlyUnfilled,
    collapsed,
    justSaved,
    castFor: cast.openFor,
    castCards: cast.cards,
    show: () => {
      visible.value = true;
      void load();
    },
    hide: () => {
      visible.value = false;
      justSaved.value = "";
      cast.reset();
    },
    toggle: () => {
      if (visible.value) {
        visible.value = false;
        justSaved.value = "";
        cast.reset();
      } else {
        visible.value = true;
        void load();
      }
    },
    load,
    toggleColumn,
    toggleCollapsed,
    saveSummary,
    saveField,
    paste,
    drop,
    openCast: cast.open,
    closeCast: cast.close,
    toggleCast: cast.toggle,
    open,
    addChapter,
  };
}
