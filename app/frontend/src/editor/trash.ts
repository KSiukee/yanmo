// 回收站：列一列、捞回来、彻底删掉。
//
// 三条分寸与别处一致：
// - 恢复与彻底删除的**语义全在核心**（整棵子树 + 父链、只对回收站里的东西开放），这里只转发；
// - **彻底删除是不可逆的**，界面必须先问一句（跟"删书"一样）；
// - 捞回来之后目录树与书架都得跟着刷新——那是别的模块的状态，通过 `onChanged` 交回去。
//
// 只认接口不认具体命令（真命令在会话层注入）：这套状态机可以脱离界面与核心单测。

import { ref, type Ref } from "vue";

import type { RestorePreview, TrashEntry } from "../api/core";
import { t } from "../locales/index.ts";

/** 名字可能为空（没起名的那一本）：显示的永远是"有话说"的名字。 */
function nameOf(title: string): string {
  return title || t("common.untitled");
}

/** 回收站的这一行叫什么。 */
export function trashLabel(entry: Pick<TrashEntry, "kind" | "title" | "work_title" | "nodes">): string {
  const title = nameOf(entry.title);
  if (entry.kind === "work") return t("trash.label_work", { title });
  const more = entry.nodes > 1 ? t("trash.label_more", { count: entry.nodes - 1 }) : "";
  return t("trash.label_node", { work: nameOf(entry.work_title), title, more });
}

/** 回收站要用的几个动作（会话层注入真命令，测试注入替身）。 */
export interface TrashTransport {
  list: () => Promise<TrashEntry[]>;
  restoreWork: (work_id: number) => Promise<number>;
  /** 恢复前的预检：会不会与同级某章重名 */
  preview: (node_id: number) => Promise<RestorePreview>;
  /** 恢复一段；`title` 是作者给的新名字（null = 照原样恢复） */
  restoreNode: (node_id: number, title: string | null) => Promise<number>;
  purgeWork: (work_id: number) => Promise<number>;
  purgeNode: (node_id: number) => Promise<number>;
  empty: () => Promise<number>;
}

export interface TrashOptions {
  transport: TrashTransport;
  /** 当前作品：万一它被彻底删了，界面得有个地方可去 */
  workId: Ref<number | null>;
  /** 彻底删掉当前那本之后回到默认落点（核心会给一本） */
  reopen: () => Promise<void>;
  /** 捞回来 / 删掉之后，目录树与书架要跟着刷新 */
  onChanged?: () => void;
  onError?: (message: string) => void;
}

/** 一次"要作者拿主意"的恢复：删掉的这一段与还活着的某章重名。 */
export interface TrashConflict {
  entry: TrashEntry;
  preview: RestorePreview;
}

export interface Trash {
  entries: Ref<TrashEntry[]>;
  visible: Ref<boolean>;
  busy: Ref<boolean>;
  /** 非空时界面要弹一次选择：照原样恢复 / 恢复并改名 / 取消 */
  conflict: Ref<TrashConflict | null>;
  /** 冲突时作者拿的主意：给新名字就改名恢复，null = 照原样恢复 */
  resolveConflict: (rename_to: string | null) => Promise<void>;
  /** 冲突时选择取消：什么都不做 */
  cancelConflict: () => void;
  /** 打开 / 收起（打开时刷新一次） */
  toggle: () => void;
  close: () => void;
  refresh: () => Promise<void>;
  /** 捞回来（书或段），返回一句给人看的交代 */
  restore: (entry: TrashEntry) => Promise<string>;
  /** 彻底删除一项（**不可恢复**），返回一句给人看的交代 */
  purge: (entry: TrashEntry) => Promise<string>;
  /** 清空回收站 */
  empty: () => Promise<void>;
}

export function useTrash(options: TrashOptions): Trash {
  const entries = ref<TrashEntry[]>([]);
  const visible = ref(false);
  const busy = ref(false);
  const conflict = ref<TrashConflict | null>(null);

  const report = (error: unknown) => {
    options.onError?.(error instanceof Error ? error.message : String(error));
  };

  async function refresh(): Promise<void> {
    try {
      entries.value = await options.transport.list();
    } catch (error) {
      report(error);
    }
  }

  /** 一次动作统一收口：忙标记 + 刷新回收站 + 通知别的模块 + 报错。 */
  async function act(op: () => Promise<string>): Promise<string> {
    if (busy.value) return "";
    busy.value = true;
    try {
      const note = await op();
      await refresh();
      options.onChanged?.();
      return note;
    } catch (error) {
      report(error);
      return "";
    } finally {
      busy.value = false;
    }
  }

  /** 清空回收站：清掉之前先看一眼"正在写的那本"在不在里面 */
  async function empty(): Promise<void> {
    const losingCurrent = entries.value.some(
      (entry) => entry.kind === "work" && entry.id === options.workId.value,
    );
    await act(async () => {
      const count = await options.transport.empty();
      if (losingCurrent) await options.reopen(); // 正在写的那本被抹了：得有地方可去
      return t("trash.emptied_count", { count });
    });
  }

  return {
    entries,
    visible,
    busy,
    toggle: () => {
      visible.value = !visible.value;
      if (visible.value) void refresh();
    },
    close: () => {
      visible.value = false;
    },
    refresh,
    conflict,
    resolveConflict: (rename_to) =>
      act(async () => {
        const pending = conflict.value;
        conflict.value = null;
        if (!pending) return "";
        const nodes = await options.transport.restoreNode(pending.entry.id, rename_to);
        const what = t("trash.what_quoted", { title: rename_to || nameOf(pending.entry.title) });
        return nodes > 1
          ? t("trash.restored_more", { what, count: nodes - 1 })
          : t("trash.restored", { what });
      }),
    cancelConflict: () => {
      conflict.value = null;
    },
    restore: (entry) =>
      act(async () => {
        if (entry.kind === "work") {
          await options.transport.restoreWork(entry.id);
          return t("trash.work_restored", { title: nameOf(entry.title) });
        }
        // 先看一眼会不会跟还活着的某章重名：**有冲突就把决定权交回作者**，
        // 不替他改名，也不悄悄恢复出两章同名（他可能正是删了旧的、又重写了这一章）
        const preview = await options.transport.preview(entry.id);
        if (preview.name_clashes.length > 0) {
          conflict.value = { entry, preview };
          return "";
        }
        const nodes = await options.transport.restoreNode(entry.id, null);
        const what = t("trash.what_quoted", { title: nameOf(entry.title) });
        return nodes > 1
          ? t("trash.restored_more", { what, count: nodes - 1 })
          : t("trash.restored", { what });
      }),
    purge: (entry) =>
      act(async () => {
        const work_id = entry.kind === "work" ? entry.id : entry.work_id;
        const nodes =
          entry.kind === "work"
            ? await options.transport.purgeWork(entry.id)
            : await options.transport.purgeNode(entry.id);
        if (work_id === options.workId.value) await options.reopen(); // 正在写的那本被抹了
        return t("trash.purged", { nodes });
      }),
    empty,
  };
}
