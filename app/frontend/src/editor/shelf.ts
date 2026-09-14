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
import { formatCaliberWords } from "./display.ts";
import { t } from "../locales/index.ts";

/** 删掉某一本之后该开哪一本：优先列表里的第一本；一本都不剩就交给核心去建默认的。 */
export function nextWorkAfterDelete(entries: ShelfEntry[], deleted: number): number | null {
  return entries.find((entry) => entry.id !== deleted)?.id ?? null;
}

/** 书架上那一行的小字：有章就报章数，单篇文章只报字数。 */
export function shelfLabel(
  entry: Pick<ShelfEntry, "chapters" | "word_count" | "char_count" | "chars_no_punct">,
  caliber: string,
): string {
  // 数字与单位都跟着作品当前的字数口径走（逐字报「字」、按词报「词」）
  const words = formatCaliberWords(entry, caliber);
  return entry.chapters > 0
    ? t("shelf.label_with_chapters", { chapters: entry.chapters, words })
    : t("shelf.label_words_only", { words });
}

/** 作品类型的显示名（核心给的是取值，显示成中文是界面的事）。 */
export function shelfKindLabel(kind: string): string {
  if (kind === "novel") return t("shelf.kind_label_novel");
  if (kind === "collection") return t("shelf.kind_label_collection");
  return t("shelf.kind_label_article");
}

/**
 * 是不是**首启那本空壳**：只有一本书，而且它既没名字、也还没写。
 *
 * 为什么需要它：核心在首启时就会建一本**无名**的书（名字留给作者起，界面显示占位），
 * 所以"书架上没有书"这个条件永远不成立——作者看到的是一片空白加一个「未命名作品」，
 * 不知道下一步该干什么。这一条把"第一次使用"变成一个**机械可判**的状态，
 * 用来给引导语与"建一本书"的入口（见 EditorPane 顶部那条提示）。
 *
 * 判据故意宽松（书名空 + 一个字都没有 → 就是刚建好、还没动过）：作者只要写了字、
 * 或者建了第二本，它自己就不成立了——不需要额外记"有没有看过引导"。
 */
export function looksLikeFirstRun(entries: ShelfEntry[]): boolean {
  if (entries.length !== 1) return false;
  const only = entries[0];
  return only.title.trim() === "" && only.word_count === 0;
}

/** 建书页填的那几样：书名、类型、简介、卷/章命名规则。 */
export interface NewWorkDraft {
  kind: string;
  title: string;
  /** 作品简介（投稿包的大纲要用它）；空串 = 先不写 */
  summary: string;
  /** 卷 / 章的命名规则（`NamingStyle` 的稳定代码）；`null` = **跟随设置**（不写这本书的覆盖） */
  naming: string | null;
}

/** 书架要用的四个动作（会话层注入真命令，测试注入替身）。 */
export interface ShelfTransport {
  list: () => Promise<ShelfEntry[]>;
  create: (kind: string, title: string) => Promise<number>;
  rename: (work_id: number, title: string) => Promise<void>;
  remove: (work_id: number) => Promise<void>;
  /** 导出成文件（txt 分章 / json 单文件），返回落点 */
  export: (work_id: number, format: string) => Promise<ExportAck>;
  /** 写作品简介（投稿包的大纲要用它） */
  writeSummary: (work_id: number, summary: string) => Promise<void>;
  /** 把"这本书用哪套卷/章命名规则"写成它的覆盖（留 null 就是跟随设置，不调它） */
  writeNaming: (work_id: number, naming: string) => Promise<void>;
}

/** 「编辑作品」表单填的那两样：书名与简介。
 *
 * 类型与命名规则**不在编辑表单里改**——它们各有自己的改法：类型决定目录长什么样，
 * 命名规则走「整本换写法」的预览确认流程（两套改法迟早打架）。
 */
export interface EditWorkDraft {
  title: string;
  summary: string;
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
  /** 建一本新书：**先建、再补简介与这本书的命名规则**，最后切过去开写 */
  create: (draft: NewWorkDraft) => Promise<void>;
  rename: (work_id: number, title: string) => Promise<void>;
  remove: (work_id: number) => Promise<void>;
  open: (work_id: number) => Promise<void>;
  /** 导出一本书（txt / json）；落点会写进 note */
  export: (work_id: number, format: string) => Promise<void>;
  /** 存一本书的简介（投稿包的大纲要用它） */
  saveSummary: (work_id: number, summary: string) => Promise<void>;
  /** 编辑作品（改名 + 简介，一次落好）；书名空着就**不提交**，当场说清 */
  edit: (work_id: number, draft: EditWorkDraft) => Promise<void>;
  /**
   * 作品表单（新建 / 编辑）现在开着哪一种。
   *
   * 放在这一层而不是某个弹窗里：**书架与正文区都要能打开它**
   * （书架卡片上的「编辑」、首启引导那条提示上的「建一本书」）。
   */
  form: Ref<WorkForm | null>;
  openCreate: () => void;
  openEdit: (entry: ShelfEntry) => void;
  closeForm: () => void;
}

/** 作品表单的两种用法。 */
export type WorkForm = { mode: "create" } | { mode: "edit"; entry: ShelfEntry };

export function useShelf(options: ShelfOptions): Shelf {
  const entries = ref<ShelfEntry[]>([]);
  const visible = ref(false);
  const busy = ref(false);
  const note = ref("");
  const form = ref<WorkForm | null>(null);

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
    create: (draft) =>
      act(async () => {
        const work_id = await options.transport.create(draft.kind, draft.title);
        // 建书页上填的简介与命名规则**一次落好**：作者填完就不用再去别处补
        if (draft.summary.trim()) {
          await options.transport.writeSummary(work_id, draft.summary.trim());
        }
        if (draft.naming !== null) {
          await options.transport.writeNaming(work_id, draft.naming);
        }
        if (await options.openWork(work_id)) visible.value = false;
      }),
    rename: (work_id, title) =>
      act(async () => {
        await options.transport.rename(work_id, title);
      }),
    saveSummary: (work_id, summary) =>
      act(async () => {
        await options.transport.writeSummary(work_id, summary);
      }),
    edit: (work_id, draft) =>
      act(async () => {
        const title = draft.title.trim();
        // 空书名**不进核心**：核心会回 work.title_empty，而那是"写错地方"的错，
        // 不是作者做错了什么——在表单这一层就拦住，并给一句他看得懂的话。
        if (!title) {
          throw new Error(t("shelf.title_required"));
        }
        await options.transport.rename(work_id, title);
        await options.transport.writeSummary(work_id, draft.summary.trim());
      }),
    form,
    openCreate: () => {
      form.value = { mode: "create" };
    },
    openEdit: (entry) => {
      form.value = { mode: "edit", entry };
    },
    closeForm: () => {
      form.value = null;
    },
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
        const cleaned =
          ack.removed > 0 ? t("shelf.export_note_cleaned", { removed: ack.removed }) : "";
        note.value = t("shelf.export_note", {
          title: entry?.title || t("shelf.export_untitled"),
          files: ack.files,
          cleaned,
          path: ack.path,
        });
      } catch (error) {
        report(error);
      } finally {
        busy.value = false;
      }
    },
  };
}
