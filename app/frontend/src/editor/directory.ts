// 目录树接线：把「树的状态」接到「编辑会话」上。
//
// 树本身的状态机在 ./tree.ts（不认界面、也不认具体命令）；这里只管三件"什么时候做什么"：
// 换了作品就重开一棵树、落盘了就把那一行的字数改掉（不重拉目录）、点一行就切章。
//
// 与编辑会话的分工：**切章、落盘、关窗都归会话**；目录只负责"看得见、点得动、拖得走"。

import { ref, watch, type Ref } from "vue";

import {
  treeAncestors,
  treeChildren,
  treeCreateNode,
  treeMoveNode,
  treeRenameNode,
  treeSetVolumeTarget,
  treeVolumeTarget,
} from "../api/core";
import type { AutosaveState } from "./autosave";
import { DirectoryTree, type TreeRow } from "./tree";

export interface DirectoryOptions {
  /** 当前作品（换作品＝换一棵树） */
  workId: Ref<number | null>;
  /** 当前正在编辑的节点（高亮那一行 + 字数就地更新认它） */
  currentNodeId: Ref<number | null>;
  /** 落盘状态：字数变了就更新那一行 */
  saveState: Ref<AutosaveState>;
  /** 点一行就切过去（切章纪律在会话层：先落盘再切） */
  openChapter: (node_id: number) => Promise<void>;
  onError?: (message: string) => void;
}

export interface Directory {
  rows: Ref<TreeRow[]>;
  /** 当前章（界面据此高亮；**目录自己的状态不掺进来**） */
  current: Ref<number | null>;
  /** 每卷目标章数（作者设过才有）：目录里「本卷 12/30 章」的分母 */
  volumeTarget: Ref<number | null>;
  /** 设定 / 清除每卷目标章数（null = 清掉） */
  setVolumeTarget: (chapters: number | null) => Promise<void>;
  toggle: (node_id: number) => Promise<void>;
  select: (node_id: number) => Promise<void>;
  rename: (node_id: number, title: string) => Promise<void>;
  move: (node_id: number, parent_id: number | null, index: number) => Promise<void>;
  /** 新建节点（卷 / 章 / …）；失败返回 null，界面不必自己兜错 */
  create: (parent_id: number | null, kind: string, title: string) => Promise<number | null>;
  /** 拖拽前问一句：这个落点能不能放（不许拖进自己的子树） */
  canDrop: (node_id: number, parent_id: number | null) => boolean;
  /** 别处改了结构（例如新建章节走的是编辑会话那条路）之后重拉可见的层 */
  refresh: () => Promise<void>;
}

export function useDirectory(options: DirectoryOptions): Directory {
  // 真命令在这里注入：树的状态机本身不认核心，可以脱离界面单测
  const tree = new DirectoryTree({
    children: treeChildren,
    ancestors: treeAncestors,
    create: treeCreateNode,
    rename: treeRenameNode,
    move: treeMoveNode,
  });
  const rows = ref<TreeRow[]>([]);
  const volumeTarget = ref<number | null>(null);
  const sync = () => {
    rows.value = tree.rows();
  };
  const report = (error: unknown) => {
    options.onError?.(error instanceof Error ? error.message : String(error));
  };

  /** 每次结构动作后都重新摊平一次——**界面永远照着同一份可见行画**。 */
  const act = async (op: () => Promise<unknown>) => {
    try {
      await op();
    } catch (error) {
      report(error);
    } finally {
      sync();
    }
  };

  /** 作品那一棵树打开（含根层拉取）完成了吗——决定"定位当前章"由哪条路做 */
  let opened = false;

  watch(
    options.workId,
    (work_id) => {
      opened = false;
      if (work_id === null) return;
      void act(async () => {
        await tree.openWork(work_id);
        opened = true;
        // 打开作品就定位到正在写的那一章：章在收起的卷里也不会"看不见自己"
        const node_id = options.currentNodeId.value;
        if (node_id !== null) await tree.reveal(node_id);
        volumeTarget.value = await loadVolumeTarget(work_id);
      });
    },
    { immediate: true },
  );

  // 切到别的卷里的章：把那条路摊开（作品还没打开完时交给上面那条一并做）
  watch(options.currentNodeId, (node_id) => {
    if (node_id === null || !opened) return;
    void act(() => tree.reveal(node_id));
  });

  // 落盘成功后只改那一行的字数：目录不为几个字重拉一次
  watch(options.saveState, (state) => {
    const node_id = options.currentNodeId.value;
    if (node_id === null || state.status !== "saved") return;
    tree.applyWordCount(node_id, state.word_count, state.char_count > 0);
    sync();
  });

  async function create(
    parent_id: number | null,
    kind: string,
    title: string,
  ): Promise<number | null> {
    try {
      return await tree.create(parent_id, kind, title);
    } catch (error) {
      report(error);
      return null;
    } finally {
      sync();
    }
  }

  /** 读卷长：读不到就当"没设过"——这只是行小字，不该挡住目录 */
  async function loadVolumeTarget(work_id: number): Promise<number | null> {
    try {
      return await treeVolumeTarget(work_id);
    } catch {
      return null;
    }
  }

  /** 写卷长：写进去再回读一次，界面显示的永远是库里那份 */
  async function setVolumeTarget(chapters: number | null): Promise<void> {
    const work_id = options.workId.value;
    if (work_id === null) return;
    await act(async () => {
      const next = chapters !== null && chapters > 0 ? chapters : null;
      await treeSetVolumeTarget(work_id, next);
      volumeTarget.value = await loadVolumeTarget(work_id);
    });
  }

  return {
    rows,
    current: options.currentNodeId,
    volumeTarget,
    setVolumeTarget,
    toggle: (node_id) => act(() => tree.toggle(node_id)),
    select: (node_id) => options.openChapter(node_id),
    rename: (node_id, title) => act(() => tree.rename(node_id, title)),
    move: (node_id, parent_id, index) => act(() => tree.move(node_id, parent_id, index)),
    create,
    canDrop: (node_id, parent_id) =>
      parent_id === null || (parent_id !== node_id && !tree.isDescendant(node_id, parent_id)),
    refresh: () => act(() => tree.reloadVisible()),
  };
}
