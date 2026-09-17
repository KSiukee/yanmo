// 章节切换：**先落盘，再换章**；一个编辑器实例从头用到尾。
//
// 三条不变量（就是"只挂当前章、绝不扛全本"的落地）：
// 1. 切之前必须把当前章落盘干净（并记下光标）；**没落干净就不许切**——宁可停在原地；
// 2. 切换期间重复请求一律忽略（连点"下一章"不会把两章搅在一起）；
// 3. 目标章取不到（被删了 / 不是正文节点）→ 停在原章，旧内容与旧控制器都不动，只报错。

import type { EditorCursor, EditorSnapshot } from "../api/core";
import type { Autosave } from "./autosave";

export type SwitchOutcome = "switched" | "blocked" | "ignored";

export interface SwitchDeps {
  /** 当前章的落盘控制器；返回 null 表示还没挂章 */
  autosave: () => Autosave | null;
  /** 当前光标与滚动位置（切走前要记下来） */
  currentCursor: () => EditorCursor | null;
  /** 记光标 */
  saveCursor: (node_id: number, cursor: EditorCursor) => Promise<void>;
  /** 取目标章（核心会校验：不能编辑就直接抛错） */
  loadChapter: (node_id: number) => Promise<EditorSnapshot>;
  /** 把这一章放进编辑器——**换内容，不换实例** */
  applyChapter: (snapshot: EditorSnapshot) => void;
  /** 为这一章起新的落盘控制器（旧的要先 dispose，别让两个控制器同时往库里写） */
  startAutosave: (snapshot: EditorSnapshot) => void;
  onError?: (message: string) => void;
}

export class ChapterSwitch {
  // 注意：这里显式声明字段而不用构造函数参数属性——
  // 测试用 Node 的类型剥离模式跑 TS，它不支持参数属性（工具与测试要能对得上）。
  private readonly deps: SwitchDeps;
  private busy = false;

  constructor(deps: SwitchDeps) {
    this.deps = deps;
  }

  /**
   * 切到指定章节。
   *
   * 也可以直接给"已经拿到手的那一章"（例如刚新建出来的）——两条路共用同一套
   * 「先落盘、记光标，再换内容、换控制器」的流程，免得新建路径自己写一遍。
   */
  async to(target: number | EditorSnapshot): Promise<SwitchOutcome> {
    if (this.busy) return "ignored";
    const current = this.deps.autosave();
    const target_id = typeof target === "number" ? target : target.node_id;
    if (current && current.node_id === target_id) return "ignored"; // 已经在这一章

    this.busy = true;
    try {
      // ① 先落盘 + 记光标：这一步不成功就**不切**
      if (current) {
        await current.flush();
        const cursor = this.deps.currentCursor();
        if (cursor) await this.deps.saveCursor(current.node_id, cursor);
      }
      // ② 取新章（核心校验），失败就停在原章
      const snapshot = typeof target === "number" ? await this.deps.loadChapter(target) : target;
      // ③ 换内容 + 换控制器（编辑器实例不动，全本永远不会同时挂进来）
      this.deps.applyChapter(snapshot);
      this.deps.startAutosave(snapshot);
      return "switched";
    } catch (error) {
      this.deps.onError?.(error instanceof Error ? error.message : String(error));
      return "blocked";
    } finally {
      this.busy = false;
    }
  }

  /**
   * **重新读一遍当前这一章**（库里的正文被换过之后用：磁盘上那份刚被收进库里）。
   *
   * 与 [`to`] 的两点不同，都是有意的：
   * - **不落盘**：手上这份若是旧的，就该被库里那份顶掉——落盘会把刚收进来的字又盖回去；
   * - 不检查"已经在同一章"：要的就是重读同一章。
   * 取不到（章被删了 / 不是正文节点）时停在原地，只报错——旧内容与旧控制器都不动。
   */
  async reload(): Promise<boolean> {
    if (this.busy) return false;
    const current = this.deps.autosave();
    if (!current) return false;
    const node_id = current.node_id;
    this.busy = true;
    try {
      const snapshot = await this.deps.loadChapter(node_id);
      this.deps.applyChapter(snapshot);
      this.deps.startAutosave(snapshot);
      return true;
    } catch (error) {
      this.deps.onError?.(error instanceof Error ? error.message : String(error));
      return false;
    } finally {
      this.busy = false;
    }
  }

  /** 正在切吗（界面据此禁用按钮，防连点） */
  get switching(): boolean {
    return this.busy;
  }
}
