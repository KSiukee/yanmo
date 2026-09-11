// 书架：列书、建书、改名、删书、切书。
//
// 分寸与目录树一样：**切书的纪律在会话层**（先落盘、记光标，再换内容换控制器），
// 这里只管书架上那点事，外加"删完当前这本该开哪一本"这种纯计算（单独抽出来好测）。
//
// 一条硬要求：**「当前作品」不许做成全局单例**。当前是哪一本由会话持有、
// 每本书自己的"读到哪了"由核心按书分键记——所以切来切去不会互相踩。
//
// 这里只认接口不认具体命令（真命令在会话层注入）：书架的状态机可以脱离界面与核心单测。

import { ref, type Ref } from "vue";

import type { ExportAck, ShelfEntry } from "../api/core";
import { formatWords } from "./display.ts";

/** 删掉某一本之后该开哪一本：优先列表里的第一本；一本都不剩就交给核心去建默认的。 */
export function nextWorkAfterDelete(entries: ShelfEntry[], deleted: number): number | null {
  return entries.find((entry) => entry.id !== deleted)?.id ?? null;
}

/** 书架上那一行的小字：有章就报章数，单篇文章只报字数。 */
export function shelfLabel(entry: Pick<ShelfEntry, "chapters" | "word_count">): string {
  return entry.chapters > 0
    ? `${entry.chapters} 章 · ${formatWords(entry.word_count)} 字`
    : `${formatWords(entry.word_count)} 字`;
}

/** 作品类型的显示名（核心给的是取值，显示成中文是界面的事）。 */
export function shelfKindLabel(kind: string): string {
  if (kind === "novel") return "长篇";
  if (kind === "collection") return "短篇集";
  return "单篇";
}

/** 书架要用的四个动作（会话层注入真命令，测试注入替身）。 */
export interface ShelfTransport {
  list: () => Promise<ShelfEntry[]>;
  create: (kind: string, title: string) => Promise<number>;
  rename: (work_id: number, title: string) => Promise<void>;
  remove: (work_id: number) => Promise<void>;
  /** 导出成文件（txt 分章 / json 单文件），返回落点 */
  export: (work_id: number, format: string) => Promise<ExportAck>;
}

export interface ShelfOptions {
  transport: ShelfTransport;
  /** 当前作品：列表里给它标一下"正在写这本" */
  workId: Ref<number | null>;
  /**
   * 切到某一本书；`null` = 回到默认落点（一本都没有时核心会建一本）。
   * 返回是否真的切过去了（没切成功就别关面板）。
   */
  openWork: (work_id: number | null) => Promise<boolean>;
  /** 删书之前先把手上这一章落盘（存不下去就别删） */
  beforeRemove?: () => Promise<void>;
  onError?: (message: string) => void;
}

export interface Shelf {
  entries: Ref<ShelfEntry[]>;
  visible: Ref<boolean>;
  busy: Ref<boolean>;
  /** 上一次动作的交代（"导出到哪儿了"之类） */
  note: Ref<string>;
  /** 打开 / 收起书架（打开时顺手刷新一次） */
  toggle: () => void;
  close: () => void;
  refresh: () => Promise<void>;
  create: (kind: string, title: string) => Promise<void>;
  rename: (work_id: number, title: string) => Promise<void>;
  remove: (work_id: number) => Promise<void>;
  open: (work_id: number) => Promise<void>;
  /** 导出一本书（txt / json）；落点会写进 note */
  export: (work_id: number, format: string) => Promise<void>;
}

export function useShelf(options: ShelfOptions): Shelf {
  const entries = ref<ShelfEntry[]>([]);
  const visible = ref(false);
  const busy = ref(false);
  const note = ref("");

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

  /** 把一次"会改书架的动作"包起来：忙标记 + 刷新 + 报错，一处收口。 */
  async function act(op: () => Promise<void>): Promise<void> {
    if (busy.value) return;
    busy.value = true;
    try {
      await op();
      await refresh();
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  async function open(work_id: number): Promise<void> {
    if (work_id === options.workId.value) {
      visible.value = false; // 已经在这本书里：点一下就是"回到正文"
      return;
    }
    await act(async () => {
      if (await options.openWork(work_id)) visible.value = false;
    });
  }

  return {
    entries,
    visible,
    busy,
    note,
    toggle: () => {
      visible.value = !visible.value;
      if (visible.value) void refresh();
    },
    close: () => {
      visible.value = false;
    },
    refresh,
    open,
    create: (kind, title) =>
      act(async () => {
        const work_id = await options.transport.create(kind, title);
        if (await options.openWork(work_id)) visible.value = false;
      }),
    rename: (work_id, title) =>
      act(async () => {
        await options.transport.rename(work_id, title);
      }),
    remove: (work_id) =>
      act(async () => {
        await options.beforeRemove?.();
        await options.transport.remove(work_id);
        if (work_id !== options.workId.value) return; // 删的不是当前这本：留在书架上就行
        const next = nextWorkAfterDelete(entries.value, work_id);
        await options.openWork(next); // next 为 null 时核心会给一本默认的
      }),
    export: async (work_id, format) => {
      const entry = entries.value.find((item) => item.id === work_id);
      busy.value = true;
      try {
        const ack = await options.transport.export(work_id, format);
        const cleaned = ack.removed > 0 ? `，清掉 ${ack.removed} 个旧文件` : "";
        note.value = `《${entry?.title ?? "这本书"}》已导出 ${ack.files} 个文件${cleaned}：${ack.path}`;
      } catch (error) {
        report(error);
      } finally {
        busy.value = false;
      }
    },
  };
}
