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

/** 外观 / 写作行为偏好（每项都已经落到具体值）。 */
export interface Appearance {
  /** 打开最新一章时跳到段末并聚焦输入光标 */
  jump_to_end_on_latest: boolean;
}

/** 要改的偏好项：**只写传进来的**，没传的保持原样。 */
export interface AppearancePatch {
  jump_to_end_on_latest?: boolean;
}

/** 删章留下的一处空缺——点「+」时问那一句的依据。 */
export interface ChapterGap {
  /** 回收站里那一条（「去回收站看看」用它） */
  node_id: number;
  /** 缺在哪一层；null = 根级 */
  parent_id: number | null;
  parent_title: string | null;
  /** 第几号 */
  serial: number;
  /** 原来叫什么 */
  title: string;
  /** 什么时候删的（unix 毫秒） */
  deleted_at: number;
  /** 旧稿多少字（让作者知道"字还在"） */
  word_count: number;
}

/** 书架的一行：作品 + 它的规模。 */
export interface ShelfEntry {
  id: number;
  /** 「novel / collection / article」——界面只用来显示，不假设行为差异 */
  kind: string;
  title: string;
  /** 这本书里章的个数（单篇文章是 0，此时只报字数） */
  chapters: number;
  /** 字数合计 */
  word_count: number;
  /** 最近打开（unix 毫秒）；从没打开过是 null */
  opened_at: number | null;
  created_at: number;
  updated_at: number;
}

/** 同级里与它重名的那一个（恢复前会摆给作者看）。 */
export interface NameClash {
  id: number;
  title: string;
  word_count: number;
}

/** 恢复**之前**的交代：回到哪、会不会与谁重名。 */
export interface RestorePreview {
  work_id: number;
  work_title: string;
  /** 原来的父级；null = 根级 */
  parent_title: string | null;
  /** 原来在第几位（从 1 起） */
  index: number;
  name_clashes: NameClash[];
}

/** 一次导出的回执。 */
export interface ExportAck {
  /** 导到哪个文件夹（界面只展示，不碰文件系统） */
  path: string;
  files: number;
  /** 顺手清掉了几个上次导出留下的旧文件 */
  removed: number;
}

/** 回收站里的一项。 */
export interface TrashEntry {
  /** 「work」= 整本书；「node」= 书里被删的一段 */
  kind: string;
  id: number;
  title: string;
  work_id: number;
  work_title: string;
  deleted_at: number;
  /** 跟着一起进来 / 会一起回去的节点数（含它自己） */
  nodes: number;
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
  openWorkTarget: "open_work_target",
  createChapter: "create_chapter",
  chapterNeighbors: "chapter_neighbors",
  treeChildren: "tree_children",
  treeAncestors: "tree_ancestors",
  treeCreateNode: "tree_create_node",
  treeRenameNode: "tree_rename_node",
  treeMoveNode: "tree_move_node",
  treeDeleteNode: "tree_delete_node",
  treeVolumeTarget: "tree_volume_target",
  treeSetVolumeTarget: "tree_set_volume_target",
  treeGapCheck: "tree_gap_check",
  treeGapAnswer: "tree_gap_answer",
  treeFillGap: "tree_fill_gap",
  appearanceRead: "appearance_read",
  appearanceWrite: "appearance_write",
  appearanceReset: "appearance_reset",
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
  listShelf: "list_shelf",
  createWork: "create_work",
  renameWork: "rename_work",
  deleteWork: "delete_work",
  listTrash: "list_trash",
  restoreWork: "restore_work",
  restoreNode: "restore_node",
  restorePreview: "restore_preview",
  purgeNode: "purge_node",
  purgeWork: "purge_work",
  emptyTrash: "empty_trash",
  exportWork: "export_work",
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

/** 切到某一本书：落点是它上次写的那一章（记不起来就是第一章）。 */
export const openWorkTarget = (work_id: number) =>
  call<EditorSnapshot>(COMMANDS.openWorkTarget, { work_id });

/** 书架：最近打开的书在前，带每本书的章数与字数。 */
export const listShelf = () => call<ShelfEntry[]>(COMMANDS.listShelf);

/** 新建一本书，返回它的 id。 */
export const createWork = (kind: string, title: string) =>
  call<number>(COMMANDS.createWork, { kind, title });

/** 给书改名。 */
export const renameWork = (work_id: number, title: string) =>
  call<void>(COMMANDS.renameWork, { work_id, title });

/** 删掉一本书（软删除，正文与历史都留着）。 */
export const deleteWork = (work_id: number) => call<void>(COMMANDS.deleteWork, { work_id });

/** 回收站：整本书在前，书里被删的段在后。 */
export const listTrash = () => call<TrashEntry[]>(COMMANDS.listTrash);

/** 从回收站恢复一本书。 */
export const restoreWork = (work_id: number) =>
  call<number>(COMMANDS.restoreWork, { work_id });

/** 恢复前先看一眼：会不会与同级某章重名（有冲突时界面要让作者拿主意）。 */
export const restorePreview = (node_id: number) =>
  call<RestorePreview>(COMMANDS.restorePreview, { node_id });

/**
 * 从回收站恢复一段（整棵子树 + 还在回收站里的父链），返回恢复的节点数。
 *
 * `title` 是作者给的新名字（留空 = 照原样恢复）；**名字永远由他给，系统不替他起**。
 */
export const restoreNode = (node_id: number, title: string | null) =>
  call<number>(COMMANDS.restoreNode, { node_id, title });

/** 彻底删除一段（**不可恢复**），返回删掉的节点数。 */
export const purgeNode = (node_id: number) => call<number>(COMMANDS.purgeNode, { node_id });

/** 彻底删除一本书（**不可恢复**），返回删掉的节点数。 */
export const purgeWork = (work_id: number) => call<number>(COMMANDS.purgeWork, { work_id });

/** 清空回收站，返回清掉的项数。 */
export const emptyTrash = () => call<number>(COMMANDS.emptyTrash);

/** 把一本书导出成 txt（分章）或 json（单文件）；同样的内容不会重复写。 */
export const exportWork = (work_id: number, format: string) =>
  call<ExportAck>(COMMANDS.exportWork, { work_id, format });

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

/** 目录树：删掉一个节点（**软删除**，连同子树进回收站，能捞回来）。 */
export const treeDeleteNode = (node_id: number) =>
  call<number>(COMMANDS.treeDeleteNode, { node_id });

/** 每卷目标章数（目录里那行「本卷 12/30 章」的分母）；没设过是 null。 */
export const treeVolumeTarget = (work_id: number) =>
  call<number | null>(COMMANDS.treeVolumeTarget, { work_id });

/** 设定 / 清除每卷目标章数（null = 清掉）。 */
export const treeSetVolumeTarget = (work_id: number, chapters: number | null) =>
  call<void>(COMMANDS.treeSetVolumeTarget, { work_id, chapters });

/** 删章路标：这一层现在该不该问一句（不该问就是 null）。 */
export const treeGapCheck = (work_id: number, parent_id: number | null) =>
  call<ChapterGap | null>(COMMANDS.treeGapCheck, { work_id, parent_id });

/** 删章路标：记下对某处空缺的答复（deferred = 稍后再说 / ignored = 不用了）。 */
export const treeGapAnswer = (node_id: number, answer: "deferred" | "ignored") =>
  call<void>(COMMANDS.treeGapAnswer, { node_id, answer });

/** 删章路标：补写——在原来的层、用原来的名字与位置新建空章，返回新章 id。 */
export const treeFillGap = (node_id: number) =>
  call<number>(COMMANDS.treeFillGap, { node_id });

/** 读外观 / 写作行为偏好；`work_id` 给 null 就是只看全局那份。 */
export const readAppearance = (work_id: number | null) =>
  call<Appearance>(COMMANDS.appearanceRead, { work_id });

/** 改偏好（稀疏合并），返回**写完回读**的那一份。 */
export const writeAppearance = (work_id: number | null, patch: AppearancePatch) =>
  call<Appearance>(COMMANDS.appearanceWrite, { work_id, patch });

/** 偏好回到默认（书的覆盖则是"回到继承全局"）。 */
export const resetAppearance = (work_id: number | null) =>
  call<Appearance>(COMMANDS.appearanceReset, { work_id });

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
