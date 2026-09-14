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
  treeDeleteNode,
  treeMoveNode,
  treeRenameNode,
  treeSetVolumeTarget,
  treeVolumeTarget,
} from "../api/core";
import { volumeClose, volumeDissolve, volumeOffer, volumePlan } from "../api/volumes";
import type { VolumePlan } from "../api/volumes";
import type { AutosaveState } from "./autosave";
import { DirectoryTree, type TreeRow } from "./tree";
import { useVolumes, type Volumes } from "./volumes";

export interface DirectoryOptions {
  /** 当前作品（换作品＝换一棵树） */
  workId: Ref<number | null>;
  /** 当前正在编辑的节点（高亮那一行 + 字数就地更新认它） */
  currentNodeId: Ref<number | null>;
  /** 落盘状态：字数变了就更新那一行 */
  saveState: Ref<AutosaveState>;
  /** 点一行就切过去（切章纪律在会话层：先落盘再切） */
  openChapter: (node_id: number) => Promise<void>;
  /** 打开"刚新建/补写"的那一章（成卷时核心顺手起的第一章走这条，要接着写） */
  openFresh: (node_id: number) => Promise<void>;
  onError?: (message: string) => void;
}

export interface Directory {
  rows: Ref<TreeRow[]>;
  /** 当前章（界面据此高亮；**目录自己的状态不掺进来**） */
  current: Ref<number | null>;
  /** 每卷目标章数（作者设过才有）：目录里「本卷 12/30 章」的分母 */
  volumeTarget: Ref<number | null>;
  /**
   * 分卷口径：作者设的、从他收好的卷学到的、以及**真正在用的**那个阈值。
   *
   * 目录里的分母用 `effective`——**学到的那个数说了算**，不然会出现
   * 「本卷 26/30 章」旁边却问"要不要在 27 章收卷"这种自相矛盾的画面。
   */
  plan: Ref<VolumePlan | null>;
  /** 设定 / 清除每卷目标章数（null = 清掉） */
  setVolumeTarget: (chapters: number | null) => Promise<void>;
  toggle: (node_id: number) => Promise<void>;
  select: (node_id: number) => Promise<void>;
  rename: (node_id: number, title: string) => Promise<void>;
  move: (node_id: number, parent_id: number | null, index: number) => Promise<void>;
  /** 删掉一个节点（软删，进回收站）；返回连带删掉了几项 */
  remove: (node_id: number) => Promise<number | null>;
  /** `node_id` 是不是 `ancestor_id` 自己或它的子孙 */
  contains: (ancestor_id: number, node_id: number) => boolean;
  /** 新建节点（卷 / 章 / …）；失败返回 null，界面不必自己兜错 */
  create: (parent_id: number | null, kind: string, title: string) => Promise<number | null>;
  /** 拖拽前问一句：这个落点能不能放（不许拖进自己的子树） */
  canDrop: (node_id: number, parent_id: number | null) => boolean;
  /**
   * 分卷：目录栏上「要不要在这里收卷 / 撤销」那一小段。
   *
   * 放在这里是因为**它就长在目录栏上**：提示按当前章算、收完要重拉树。
   * 状态机本身在 [`useVolumes`]（不认命令、可单测），这里只负责接线。
   */
  volumes: Volumes;
  /** 别处改了结构（例如新建章节走的是编辑会话那条路）之后重拉可见的层 */
  refresh: () => Promise<void>;
  /** 把某一章在树上露出来（成卷之后新卷是收着的，得摊开才看得见里面的章） */
  reveal: (node_id: number) => Promise<void>;
}

export function useDirectory(options: DirectoryOptions): Directory {
  // 真命令在这里注入：树的状态机本身不认核心，可以脱离界面单测
  const tree = new DirectoryTree({
    children: treeChildren,
    ancestors: treeAncestors,
    create: treeCreateNode,
    rename: treeRenameNode,
    move: treeMoveNode,
    remove: treeDeleteNode,
  });
  const rows = ref<TreeRow[]>([]);
  const volumeTarget = ref<number | null>(null);
  const plan = ref<VolumePlan | null>(null);
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

  // 分卷：目录栏上「要不要在这里收卷 / 撤销」那一段的接线。
  // 提示按**当前章**算、收完要重拉树，所以它跟目录树同一处装配（状态机在 volumes.ts）
  const volumes = useVolumes({
    transport: { offer: volumeOffer, close: volumeClose, dissolve: volumeDissolve },
    afterChange: async () => {
      // 结构变了：重拉看得见的层 + 重读卷长口径，并把正在写的那一章露出来
      // （卷内收卷时收卷点之后的章整体进了新卷，不摊开就"看不见了"）
      await tree.reloadVisible();
      await reloadPlan();
      const node_id = options.currentNodeId.value;
      if (node_id !== null) await tree.reveal(node_id);
    },
    // 空卷写不了字：核心顺手起了第一章，落过去接着写（焦点策略与普通切章不同）
    openChapter: options.openFresh,
    onError: (message) => options.onError?.(message),
  });

  watch(
    options.workId,
    (work_id) => {
      opened = false;
      // 换作品：分卷的提示与"问过几次"的记忆只对当前这本书，换书就清干净
      volumes.reset();
      if (work_id === null) return;
      void act(async () => {
        await tree.openWork(work_id);
        opened = true;
        // 打开作品就定位到正在写的那一章：章在收起的卷里也不会"看不见自己"
        const node_id = options.currentNodeId.value;
        if (node_id !== null) await tree.reveal(node_id);
        volumeTarget.value = await loadVolumeTarget(work_id);
        await reloadPlan(work_id);
      });
    },
    { immediate: true },
  );

  // 切章：把那条路摊开，并问一句"这一章后面要不要收卷"
  // （作品还没打开完时，摊开交给上面那条一并做；问话只读一次库，不受它影响）
  watch(options.currentNodeId, (node_id) => {
    void volumes.consider(node_id);
    if (node_id === null || !opened) return;
    void act(() => tree.reveal(node_id));
  });

  // 落盘成功后只改那一行的字数：目录不为几个字重拉一次。
  // 三个口径一起给（树上显示哪个由作者的档位决定，见 tree.applyCounts 的说明）
  watch(options.saveState, (state) => {
    const node_id = options.currentNodeId.value;
    if (node_id === null || state.status !== "saved") return;
    tree.applyCounts(node_id, state, state.char_count > 0);
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

  /** 删节点：删完重拉看得见的层；失败返回 null */
  async function remove(node_id: number): Promise<number | null> {
    try {
      return await tree.remove(node_id);
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

  /**
   * 重读分卷口径：作者设的、从历史学到的、以及**真正在用的**那个阈值。
   *
   * 目录里的分母用 `effective`——不然会出现「本卷 26/30 章」旁边却问
   * "要不要在 27 章收卷"这种自相矛盾的画面。读不到就当"没尺子"（不影响目录本身）。
   */
  async function reloadPlan(work_id?: number): Promise<void> {
    const id = work_id ?? options.workId.value;
    if (id === null) {
      plan.value = null;
      return;
    }
    try {
      plan.value = await volumePlan(id);
    } catch {
      plan.value = null;
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
      await reloadPlan(work_id);
    });
  }

  return {
    rows,
    current: options.currentNodeId,
    volumeTarget,
    plan,
    volumes,
    setVolumeTarget,
    toggle: (node_id) => act(() => tree.toggle(node_id)),
    select: (node_id) => options.openChapter(node_id),
    rename: (node_id, title) => act(() => tree.rename(node_id, title)),
    move: (node_id, parent_id, index) => act(() => tree.move(node_id, parent_id, index)),
    remove,
    contains: (ancestor_id, node_id) => tree.contains(ancestor_id, node_id),
    create,
    canDrop: (node_id, parent_id) =>
      parent_id === null || (parent_id !== node_id && !tree.isDescendant(node_id, parent_id)),
    // 重拉看得见的层 + 重读卷长口径：成卷 / 撤卷会改历史，分母得跟着变
    refresh: () =>
      act(async () => {
        await tree.reloadVisible();
        await reloadPlan();
      }),
    reveal: (node_id) => act(() => tree.reveal(node_id)),
  };
}
