// 回收站：列一列、捞回来、彻底删掉。
//
// 三条分寸与别处一致：
// - 恢复与彻底删除的**语义全在核心**（整棵子树 + 父链、只对回收站里的东西开放），这里只转发；
// - **彻底删除是不可逆的**，界面必须先问一句（跟"删书"一样）；
// - 捞回来之后目录树与书架都得跟着刷新——那是别的模块的状态，通过 `onChanged` 交回去。
//
// 只认接口不认具体命令（真命令在会话层注入）：这套状态机可以脱离界面与核心单测。

import { ref, type Ref } from "vue";

import type { TrashEntry } from "../api/core";

/** 回收站的这一行叫什么。 */
export function trashLabel(entry: Pick<TrashEntry, "kind" | "title" | "work_title" | "nodes">): string {
  if (entry.kind === "work") return `整本《${entry.title}》`;
  const more = entry.nodes > 1 ? `（连带 ${entry.nodes - 1} 项）` : "";
  return `《${entry.work_title}》· ${entry.title}${more}`;
}

/** 回收站要用的几个动作（会话层注入真命令，测试注入替身）。 */
export interface TrashTransport {
  list: () => Promise<TrashEntry[]>;
  restoreWork: (work_id: number) => Promise<number>;
  restoreNode: (node_id: number) => Promise<number>;
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

export interface Trash {
  entries: Ref<TrashEntry[]>;
  visible: Ref<boolean>;
  busy: Ref<boolean>;
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
      return `回收站清空了（${count} 项）`;
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
    restore: (entry) =>
      act(async () => {
        if (entry.kind === "work") {
          await options.transport.restoreWork(entry.id);
          return `《${entry.title}》回到了书架`;
        }
        const nodes = await options.transport.restoreNode(entry.id);
        return nodes > 1 ? `「${entry.title}」连同 ${nodes - 1} 项一起回来了` : `「${entry.title}」回来了`;
      }),
    purge: (entry) =>
      act(async () => {
        const work_id = entry.kind === "work" ? entry.id : entry.work_id;
        const nodes =
          entry.kind === "work"
            ? await options.transport.purgeWork(entry.id)
            : await options.transport.purgeNode(entry.id);
        if (work_id === options.workId.value) await options.reopen(); // 正在写的那本被抹了
        return `已彻底删除（${nodes} 项），这一步没有后悔药`;
      }),
    empty,
  };
}
