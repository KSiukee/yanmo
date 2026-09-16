// 大纲表的**状态与命令编排**：读全树、逐格存、切列、折叠、只看没填的。
//
// 单独成文件的原因与别的 `*-panel.ts` 同一条：这一整块是"会发生什么"，
// `.vue` 那一份只管"长什么样"。
//
// 四条分寸：
// 1. **逐格存**：改哪一格只发那一格（一句话走 `set_node_summary`，四格一次发四格），
//    存下来的回执是**库里真有的那一份**——界面显示的永远是库里的值，不是自己回显的；
// 2. **打开才读**：它是"看一眼/改一遍"的一屏，不是常驻画面；
// 3. **列与折叠是"当下想看什么"**：不落盘（与第二栏那些开关同一条规矩）；
// 4. **不猜**：存不下就那一格报错、保持原值（绝不假装存上了）。

import { ref, watch, type Ref } from "vue";

import { asError } from "../api/errors.ts";
import { outlineRows, type OutlineRowDto } from "../api/outline.ts";
import { setNodeSummary, saveNodeFields } from "../api/core.ts";
import { DEFAULT_COLUMNS, type GridColumn } from "./grid.ts";

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
  busy: Ref<boolean>;
  /** 失败时那句已经渲染好的话（`CoreError` 走字典渲染） */
  errorText: Ref<string>;
  /** 现在露哪几列（勾选开关；不落盘） */
  columns: Ref<GridColumn[]>;
  /** 只看"要管一下"的行（没写一句话、或四格填了一半） */
  onlyUnfilled: Ref<boolean>;
  /** 收起来的卷 */
  collapsed: Ref<Set<number>>;
  /** 刚存过的那一格（界面上闪一句回执） */
  justSaved: Ref<string>;
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
      justSaved.value = `summary:${node_id}`;
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
      justSaved.value = `${field}:${node_id}`;
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
      if (visible.value) void load();
    },
  );

  return {
    visible,
    rows,
    busy,
    errorText,
    columns,
    onlyUnfilled,
    collapsed,
    justSaved,
    show: () => {
      visible.value = true;
      void load();
    },
    hide: () => {
      visible.value = false;
      justSaved.value = "";
    },
    toggle: () => {
      if (visible.value) {
        visible.value = false;
        justSaved.value = "";
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
    open,
    addChapter,
  };
}
