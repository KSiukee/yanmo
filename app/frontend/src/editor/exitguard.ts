// 关窗闸门：**存不下去就别想走**。
//
// 判定只看一件事——这一章是不是已经落到库里了（编辑器控制器的状态）。
// 其余全是补救路径：重试保存、导出到文件、用户明确"仍然退出"。
//
// 与崩溃恢复配套：这里管的是"这一次"——正常退出前一定先落盘、再留一份关窗快照；
// "上一次是不是正常退出"由核心里的会话标记负责，界面只在启动时提示一次。

import { t } from "../locales/index.ts";
import type { Autosave, AutosaveState } from "./autosave";

/** 关窗请求的处理结果。 */
export type ExitOutcome = "allowed" | "blocked";

export interface ExitGateState {
  /** 是否拦住不让走 */
  blocked: boolean;
  /** 给用户看的原因 */
  message: string;
  /** 逃生导出到的路径（用户点过"导出到文件"之后才有） */
  escapePath: string | null;
  /** 正在处理关窗（防重复点击） */
  busy: boolean;
}

export interface ExitGateDeps {
  /** 当前编辑器控制器；界面还没就绪时返回 null，此时没什么可丢的，直接放行 */
  autosave: () => Autosave | null;
  /** 正常退出收尾：留关窗快照 + 标记干净退出 */
  closeSession: (node_id: number) => Promise<unknown>;
  /** 放弃这次会话（仍然退出） */
  abandonSession: () => Promise<unknown>;
  /** 真正退出应用 */
  exitApp: () => Promise<unknown>;
  /** 逃生导出：把手上这份正文原子写到数据目录，返回路径 */
  escapeExport: (node_id: number, body: string) => Promise<string>;
  /** 当前正文（逃生导出用） */
  currentBody: () => string;
  onState?: (state: ExitGateState) => void;
}

/** 只有"已经落盘"才算安全。 */
export function isSafeToExit(state: AutosaveState): boolean {
  return state.status === "saved" || state.status === "idle";
}

function describe(state: AutosaveState): string {
  switch (state.status) {
    case "pending":
      return t("exit.reason_pending");
    case "saving":
      return t("exit.reason_saving");
    case "error":
      return t("exit.reason_error", { detail: state.detail || t("exit.unknown_reason") });
    case "desync":
      return t("exit.reason_desync", { detail: state.detail || t("exit.tried_rescue") });
    default:
      return t("exit.reason_unknown");
  }
}

export class ExitGate {
  private readonly deps: ExitGateDeps;
  private state: ExitGateState = { blocked: false, message: "", escapePath: null, busy: false };

  constructor(deps: ExitGateDeps) {
    this.deps = deps;
  }

  /** 收到关窗请求：**先逼一次落盘**，再决定放不放行。 */
  async requestExit(): Promise<ExitOutcome> {
    if (this.state.busy) return "blocked"; // 重复关窗请求：忽略，已经在处理了
    this.setState({ busy: true, blocked: false, message: "" });

    const autosave = this.deps.autosave();
    if (!autosave) {
      // 编辑器还没挂上：没有任何未落盘的内容，直接放行（也谈不上关窗快照）
      return this.leave(false);
    }

    try {
      await autosave.flush(); // 不等防抖，立刻落盘
    } catch {
      // 失败原因已经反映在 autosave 的状态里，下面统一判定
    }

    if (isSafeToExit(autosave.state())) {
      return this.leave(true);
    }

    this.setState({ blocked: true, busy: false, message: describe(autosave.state()) });
    return "blocked";
  }

  /** 阻塞之后用户点"重试保存"。 */
  async retry(): Promise<ExitOutcome> {
    this.setState({ busy: false });
    return this.requestExit();
  }

  /** 用户点"导出到文件"：把手上这份正文原子写到数据目录。 */
  async escape(): Promise<string | null> {
    const autosave = this.deps.autosave();
    if (!autosave) return null;
    this.setState({ busy: true });
    try {
      const path = await this.deps.escapeExport(autosave.node_id, this.deps.currentBody());
      this.setState({ blocked: true, busy: false, escapePath: path });
      return path;
    } catch (error) {
      this.setState({
        blocked: true,
        busy: false,
        message: t("exit.escape_failed", {
          detail: error instanceof Error ? error.message : String(error),
        }),
      });
      return null;
    }
  }

  /** 用户点"仍然退出"：把选择权交还给人，只标记干净退出（不写关窗快照）。 */
  async forceExit(): Promise<ExitOutcome> {
    try {
      await this.deps.abandonSession();
    } catch {
      // 标不上就算了——用户已经明确要走，别把人困在窗口里
    }
    return this.leave(false);
  }

  state_(): ExitGateState {
    return this.state;
  }

  /** 放行：需要收尾时先留关窗快照，再真正退出。 */
  private async leave(withCloseSnapshot: boolean): Promise<ExitOutcome> {
    const autosave = this.deps.autosave();
    if (withCloseSnapshot && autosave) {
      try {
        await this.deps.closeSession(autosave.node_id);
      } catch {
        // 快照写不上不能拦着人退出：正文此前该落的已经落了
      }
    }
    this.setState({ blocked: false, busy: true, message: "" });
    await this.deps.exitApp();
    return "allowed";
  }

  private setState(patch: Partial<ExitGateState>): void {
    this.state = { ...this.state, ...patch };
    this.deps.onState?.(this.state);
  }
}
