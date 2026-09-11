// 壳与核心之间**唯一的通道**。
//
// 纪律（不是口头约定，有机械检查盯着）：
// - 只有本目录下的文件可以 import "@tauri-apps/api"；
// - 组件一律从这里取数据，不直接 invoke；
// - 这里不出现文件系统 API，也没有「路径参数」——数据在哪由 Rust 侧决定，
//   界面拿到的只是被报告出来的结果。
//
// 命名：命令参数与返回值一律 snake_case，和 Rust 侧一致——不给"两套写法"留出错空间。
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** 引擎基本信息（编译期常量）。 */
export interface EngineInfo {
  name: string;
  version: string;
  protocol_version: number;
  data_format_version: number;
}

/** 数据落点（运行期状态，只读报告）。 */
export interface DataHome {
  path: string;
  schema_version: number;
}

/** 打开编辑器时拿到的一章。 */
export interface EditorSnapshot {
  work_id: number;
  node_id: number;
  title: string;
  /** 正文（纯文本，段落之间空行分隔） */
  body: string;
  char_count: number;
  word_count: number;
  /** 库里这份正文的内容指纹；从未写过时为空串 */
  fingerprint: string;
  /** 上次读到哪了（**只有记的正是这一章时才有**） */
  cursor: EditorCursor | null;
}

/** 光标与滚动位置。 */
export interface EditorCursor {
  anchor: number;
  head: number;
  scroll_top: number;
}

/** 导航用的章节条目。 */
export interface ChapterSummary {
  id: number;
  title: string;
  word_count: number;
}

/** 当前章的邻居与位置（上一章 / 下一章按阅读顺序，**跨卷**）。 */
export interface ChapterNeighbors {
  previous: ChapterSummary | null;
  next: ChapterSummary | null;
  index: number;
  total: number;
}

/** 目录树的一个条目——**没有正文字段**（核心的类型上就没有，懒加载不靠自觉）。 */
export interface TreeNode {
  id: number;
  parent_id: number | null;
  /** 「卷 / 章 / 节 / 单篇 / 场景卡」——**界面不假设层级**，只按取值显示 */
  kind: string;
  title: string;
  word_count: number;
  /** 已经有正文了吗（空章一眼可见） */
  has_body: boolean;
  /** 这个节点能不能编辑正文（点一下是"打开来写"还是"展开看看"） */
  holds_body: boolean;
  /** 这个节点能不能收下级（拖进来算不算数） */
  accepts_children: boolean;
  /** 下面还有没有节点（决定要不要画展开箭头） */
  has_children: boolean;
  /** 容器行的小字：本卷几章（**非容器是 0**） */
  chapter_count: number;
  /** 容器行的小字：本卷共多少字（**非容器是 0**） */
  subtree_word_count: number;
}

/** 一次落盘的回执。 */
export interface SaveAck {
  char_count: number;
  word_count: number;
  /** 落盘后库里的内容指纹（写后读回校验的比对基准） */
  fingerprint: string;
}

/** 上一次会话的交代。 */
export interface SessionNotice {
  /** 上次没有正常退出（被杀 / 崩溃） */
  unclean: boolean;
  last_node_id: number | null;
  /** 上次活动时间（unix 毫秒） */
  last_seen_at: number | null;
}

/** 退出收尾的回执。 */
export interface CloseAck {
  snapshot_written: boolean;
}

/** 逃生导出的结果。 */
export interface EscapeAck {
  /** 导出到的完整路径（界面只展示，不碰文件系统） */
  path: string;
}

/** 核心不可达：浏览器预览模式，或核心启动失败。 */
export class CoreUnavailableError extends Error {}

/** 命令白名单：新增命令先在这里登记，别在组件里裸调。 */
const COMMANDS = {
  engineInfo: "engine_info",
  dataHome: "data_home",
  exitApp: "exit_app",
  openEditorTarget: "open_editor_target",
  openChapter: "open_chapter",
  createChapter: "create_chapter",
  chapterNeighbors: "chapter_neighbors",
  treeChildren: "tree_children",
  treeAncestors: "tree_ancestors",
  treeCreateNode: "tree_create_node",
  treeRenameNode: "tree_rename_node",
  treeMoveNode: "tree_move_node",
  treeVolumeTarget: "tree_volume_target",
  treeSetVolumeTarget: "tree_set_volume_target",
  saveCursor: "save_cursor",
  saveBody: "save_body",
  bodyFingerprint: "body_fingerprint",
  emergencySnapshot: "emergency_snapshot",
  sessionReport: "session_report",
  armExitGate: "arm_exit_gate",
  ackCloseRequest: "ack_close_request",
  closeSession: "close_session",
  abandonSession: "abandon_session",
  escapeExport: "escape_export",
} as const;

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (e) {
    throw new CoreUnavailableError(typeof e === "string" ? e : String(e));
  }
}

export const readEngineInfo = () => call<EngineInfo>(COMMANDS.engineInfo);
export const readDataHome = () => call<DataHome>(COMMANDS.dataHome);

/** 当前该编辑的一章（首次运行会引导出一篇默认作品；上次被杀则回到崩前那一章）。 */
export const openEditorTarget = () => call<EditorSnapshot>(COMMANDS.openEditorTarget);

/** 切到指定章节（这一章必须真的能编辑，否则核心会明确报错）。 */
export const openChapter = (node_id: number) =>
  call<EditorSnapshot>(COMMANDS.openChapter, { node_id });

/** 在当前章后面新建一章（目录树接上前的最小入口）。 */
export const createChapter = (node_id: number, title: string) =>
  call<EditorSnapshot>(COMMANDS.createChapter, { node_id, title });

/** 上一章 / 下一章（按阅读顺序，跨卷）。 */
export const chapterNeighbors = (node_id: number) =>
  call<ChapterNeighbors>(COMMANDS.chapterNeighbors, { node_id });

/** 目录树：取某一层的子节点（**展开哪一层拉哪一层**）。 */
export const treeChildren = (work_id: number, parent_id: number | null) =>
  call<TreeNode[]>(COMMANDS.treeChildren, { work_id, parent_id });

/** 目录树：从根到该节点父级的 id 链（**一层层展开到正在写的那一章**用）。 */
export const treeAncestors = (node_id: number) => call<number[]>(COMMANDS.treeAncestors, { node_id });

/** 目录树：新建节点（卷 / 章 / …），返回新节点 id；**标题留空 = 按同层取号命名**。 */
export const treeCreateNode = (
  work_id: number,
  parent_id: number | null,
  kind: string,
  title: string,
) => call<number>(COMMANDS.treeCreateNode, { work_id, parent_id, kind, title });

/** 目录树：内联改名。 */
export const treeRenameNode = (node_id: number, title: string) =>
  call<void>(COMMANDS.treeRenameNode, { node_id, title });

/** 目录树：拖拽排序（挪到新父级的第 index 位；越界由核心夹到末尾）。 */
export const treeMoveNode = (node_id: number, parent_id: number | null, index: number) =>
  call<void>(COMMANDS.treeMoveNode, { node_id, parent_id, index });

/** 每卷目标章数（目录里那行「本卷 12/30 章」的分母）；没设过是 null。 */
export const treeVolumeTarget = (work_id: number) =>
  call<number | null>(COMMANDS.treeVolumeTarget, { work_id });

/** 设定 / 清除每卷目标章数（null = 清掉）。 */
export const treeSetVolumeTarget = (work_id: number, chapters: number | null) =>
  call<void>(COMMANDS.treeSetVolumeTarget, { work_id, chapters });

/** 记下"这一章读到哪了"（失焦 / 切章 / 关窗时调用）。 */
export const saveCursor = (node_id: number, cursor: EditorCursor) =>
  call<void>(COMMANDS.saveCursor, { node_id, ...cursor });

/** 落盘正文（防抖后调用；内容没变时核心不写库）。 */
export const saveBody = (node_id: number, body: string) =>
  call<SaveAck>(COMMANDS.saveBody, { node_id, body });

/** 库里正文的指纹——读回校验用，不搬运正文。 */
export const bodyFingerprint = (node_id: number) =>
  call<string>(COMMANDS.bodyFingerprint, { node_id });

/** 抢救：先把手上这份留成快照，再把库改回这一版。 */
export const emergencySnapshot = (node_id: number, body: string, reason: string) =>
  call<SaveAck>(COMMANDS.emergencySnapshot, { node_id, body, reason });

/** 上次会话的交代（崩溃检测）。 */
export const sessionReport = () => call<SessionNotice>(COMMANDS.sessionReport);

/** 界面已就绪：从现在起关窗会先过闸门。 */
export const armExitGate = () => call<void>(COMMANDS.armExitGate);

/** 界面回话：告诉壳"关窗通知收到了，正在处理"（壳据此停掉"界面已死"的倒计时）。 */
export const ackCloseRequest = () => call<void>(COMMANDS.ackCloseRequest);

/** 正常退出收尾：留关窗快照 + 标记干净退出。 */
export const closeSession = (node_id: number) => call<CloseAck>(COMMANDS.closeSession, { node_id });

/** 用户选择"仍然退出"：只标记干净退出。 */
export const abandonSession = () => call<void>(COMMANDS.abandonSession);

/** 逃生导出：原子写到数据目录下的逃生文件夹，返回路径。 */
export const escapeExport = (node_id: number, body: string) =>
  call<EscapeAck>(COMMANDS.escapeExport, { node_id, body });

/** 真正退出应用（只在闸门放行后调用）。 */
export const requestExit = () => call<void>(COMMANDS.exitApp);

/**
 * 关窗请求：Rust 拦下系统关窗后通知界面，由界面决定能不能走。
 *
 * 这是唯一允许"界面参与关窗决定"的通道——**存不下去就不放行**。
 */
export const onCloseRequested = (handler: () => void): Promise<UnlistenFn> =>
  listen("close-requested", () => handler());
