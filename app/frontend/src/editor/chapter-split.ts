// 拆章的**编排**：先落盘 → 切 → 切到新章 → 留下撤销的把手。
//
// 与 `chapters.ts` 里的 `ChapterSwitch` 同一条路数：**依赖显式注入**（会话把"落盘、切章、
// 收尾"那几件事递进来），于是这一段能脱离会话单独读、单独测；会话那一份文件也不必为
// 一次动作再长几十行（它本来就贴着行数上限）。
//
// 三条分寸（与任务口径一一对应）：
// - **先落盘再拆**：核心切的是库里那一份，手上没存完的字不在里面。落盘失败就**不许拆**
//   ——否则那一刀切的是旧正文，作者眼前这份字会错位（与"删东西之前先落盘"同一条纪律）；
// - **切过去、光标落在新章开头**：作者刚才把光标放在切点上，拆完就该停在那一处接着写
//   （复用"新建一章"那条切章路，不另起一套）；
// - **留下撤销的把手**：回执存进 `notice`，界面上给一次「撤销」。
//
// 本文件不认识 Vue 组件、不认识 Tauri：命令从 `api/split.ts` 来，其余全是注入。

import { ref, type Ref } from "vue";
import type { EditorSnapshot } from "../api/core.ts";
import type { SplitAck, SplitReceipt } from "../api/split.ts";
import { t } from "../locales/index.ts";

export interface SplitDeps {
  /** 现在开着的是哪一章（没开就是 `null`） */
  currentNode: () => number | null;
  /** 落盘；**拿不到成功就不许拆**（抛错即停） */
  flush: () => Promise<void>;
  /** 真去拆那一下（命令注入：单测喂替身，本文件因此不必认识 Tauri） */
  runSplit: (node_id: number, at: number, title: string) => Promise<SplitAck>;
  /** 真去撤销那一下 */
  runUndo: (receipt: SplitReceipt) => Promise<EditorSnapshot>;
  /** 切到某一章，回"切成了没有" */
  switchTo: (snapshot: EditorSnapshot) => Promise<boolean>;
  /** 一次动作之后的收尾：重取上下章、把焦点还给正文、刷目录 */
  settle: () => Promise<void>;
  /** 会话的切章闸门：拆章与撤销期间同样要锁上（不许并行两个切换） */
  busy: Ref<boolean>;
  /** 报一句失败（会话把它摆在状态栏那一行） */
  report: (message: string) => void;

  /**
   * 把当前这一章的落盘控制器**摘下来**（不是销毁）。
   *
   * 撤销拆章时，我们人正站在**那个即将被删掉的新章**上：切章那条纪律要"先落盘、记光标"，
   * 而那一刻那个节点已经没了——那一步会报「节点不存在或已删除」并把整次撤销拦下来
   * （真机走查撞出来的；"删的正是手上这一支"那条路早踩过同一个坑，见会话里的 deleteNode）。
   */
  detachCurrent: () => void;
}

export interface ChapterSplit {
  /** 刚拆完的那一次（条上给一次「撤销」）；`null` = 现在没有可撤销的 */
  notice: Ref<SplitReceipt | null>;
  /** 在**第 `at` 段之前**把当前这一章切成两章 */
  splitAt: (at: number, title: string) => Promise<boolean>;
  /** 撤销刚那一次（两章合回原章、新章进回收站） */
  undo: () => Promise<boolean>;
}

/** 失败那句话：命令抛出来的原话照带（测试与真机都能一眼看出是哪一步）。 */
function detailOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function useChapterSplit(deps: SplitDeps): ChapterSplit {
  const notice = ref<SplitReceipt | null>(null);

  async function splitAt(at: number, title: string): Promise<boolean> {
    if (deps.busy.value) return false;
    const node_id = deps.currentNode();
    if (node_id === null) return false;
    deps.busy.value = true;
    try {
      await deps.flush();
      const ack = await deps.runSplit(node_id, at, title);
      if (!(await deps.switchTo(ack.new_chapter))) return false;
      notice.value = ack.receipt;
      await deps.settle();
      return true;
    } catch (error) {
      deps.report(t("session.split_chapter_failed", { detail: detailOf(error) }));
      return false;
    } finally {
      deps.busy.value = false;
    }
  }

  async function undo(): Promise<boolean> {
    const receipt = notice.value;
    if (receipt === null || deps.busy.value) return false;
    deps.busy.value = true;
    try {
      // 顺序与"删除手上这一章"完全一样（那一处也踩过同一个坑）：
      // **先落盘**（新章里后来又写的字先存下来——它随节点进回收站，不丢） →
      // 调撤销（新章这一步就没了） → **摘掉落盘控制器**（切换流程不再往一个已删的节点上写）
      // → 切回原章。少了"摘"这一下，整次撤销会卡在「节点不存在或已删除」。
      await deps.flush();
      const restored = await deps.runUndo(receipt);
      deps.detachCurrent();
      if (!(await deps.switchTo(restored))) return false;
      notice.value = null; // 撤过了：这一条不再摆着（再拆一次会给新的把柄）
      await deps.settle();
      return true;
    } catch (error) {
      deps.report(t("session.split_undo_failed", { detail: detailOf(error) }));
      return false;
    } finally {
      deps.busy.value = false;
    }
  }

  return { notice, splitAt, undo };
}
