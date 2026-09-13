// 壳与核心之间**唯一的通道**。
//
// 纪律（不是口头约定，有机械检查盯着）：
// - 只有本目录下的文件可以 import "@tauri-apps/api"；
// - 组件一律从这里取数据，不直接 invoke；
// - 这里不出现文件系统 API，也没有「路径参数」——数据在哪由 Rust 侧决定，
//   界面拿到的只是被报告出来的结果。
//
// 命名：命令参数与返回值一律 snake_case，和 Rust 侧一致——不给"两套写法"留出错空间。
//
// 错误：Rust 侧只回**码 + 参数**（见 `src/locales/` 里的 `error.*`），
// 本文件把它们渲染成句子——**界面文案只有这一处来源**，别在组件里另拼中文。
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { asError, CoreError, CoreUnavailableError, healText } from "./errors";

// 错误类型从 `api/errors.ts` 转出：用的人照旧从网关取，不必知道它住在哪。
export { CoreError, CoreUnavailableError };

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
  /** 三个字数口径一起给：界面按作者选的那个显示（切换时不必再跑一趟核心） */
  char_count: number;
  chars_no_punct: number;
  word_count: number;
  /** 作品语言（zh / en / ja）——字数默认口径跟它走 */
  work_language: string;
  /** **落定后**的字数口径（作者选过就是它，没选过就是作品语言的默认） */
  word_caliber: string;
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
  /** 逐字（含标点） */
  char_count: number;
  /** 逐字（不含标点） */
  chars_no_punct: number;
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
  /** 容器行的小字：本卷共多少字（**非容器是 0**）——按词口径 */
  subtree_word_count: number;
  /** 同上，逐字（含标点） */
  subtree_char_count: number;
  /** 同上，逐字（不含标点） */
  subtree_chars_no_punct: number;
}

/** 外观 / 写作行为偏好（每项都已经落到具体值）。 */
export interface Appearance {
  /** 打开最新一章时跳到段末并聚焦输入光标 */
  jump_to_end_on_latest: boolean;
  /** 作者选过的字数口径（chars / chars_no_punct / words）；null = 没选过，跟作品语言走 */
  word_count_caliber: string | null;
  /** 引号用哪一套（curly / corner）：排版清理按它统一引号 */
  quote_style: string;
}

/** 要改的偏好项：**只写传进来的**，没传的保持原样。 */
export interface AppearancePatch {
  jump_to_end_on_latest?: boolean;
  word_count_caliber?: string;
  quote_style?: string;
}

/** 一条排版规则：稳定代码 + 风险档（safe 默认勾 / careful 自己勾 / style 默认不动）。 */
export interface TypesetRule {
  code: string;
  tier: string;
}

/** 扫描出来的一处建议改动——**只是建议**，勾了才改。 */
export interface TypesetChange {
  /** 哪条规则提出来的（名字与说明在字典的 typeset.rule.* 里） */
  rule: string;
  /** 改前那一小段（原文里的样子；插入类改动这里是空串） */
  before: string;
  /** 改后（删除类改动这里是空串） */
  after: string;
  /** 落在第几段（从 1 起） */
  paragraph: number;
  context_before: string;
  context_after: string;
}

/** 一处**只报告、不给改法**的提醒（例如引号缺一半：补哪边只有作者知道）。 */
export interface TypesetNotice {
  /** 哪个检查提出来的（名字在字典的 typeset.notice.* 里） */
  rule: string;
  /** 哪一对符号（quote_double / corner / paren / title…），名字在 typeset.mark.* 里 */
  mark: string;
  /** unclosed（开着的没关）/ unopened（关着的没有开），句子在 typeset.side.* 里 */
  side: string;
  /** 落在第几段（从 1 起） */
  paragraph: number;
  context_before: string;
  context_after: string;
}

/** 一次扫描的结果：能改的与只能提醒的，分开放。 */
export interface TypesetReport {
  changes: TypesetChange[];
  notices: TypesetNotice[];
}

/** 扫描选项（现在是引号风格一种）。 */
export interface TypesetOptions {
  quote_style: string;
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
  /** 字数合计（按词口径） */
  word_count: number;
  /** 同上，逐字（含标点） */
  char_count: number;
  /** 同上，逐字（不含标点） */
  chars_no_punct: number;
  /** 最近打开（unix 毫秒）；从没打开过是 null */
  opened_at: number | null;
  created_at: number;
  updated_at: number;
}

/** 一个备份目标（作者勾的一处落点）。 */
export interface BackupTarget {
  path: string;
  /** 卷序列号——**识别"哪块盘"用它，不用盘符**（换 USB 口盘符会变） */
  volume_id: string;
  volume_label: string;
  removable: boolean;
}

/** 备份偏好。 */
export interface BackupConfig {
  targets: BackupTarget[];
  /** 每个目标保留最近几份成功的备份 */
  keep: number;
  auto_on_start: boolean;
  auto_on_close: boolean;
  /** 作者拒绝过那条"插个盘吧"的小条 */
  tip_dismissed: boolean;
}

/** 一块盘（界面给作者列勾选项）。 */
export interface BackupVolume {
  root: string;
  label: string;
  volume_id: string;
  removable: boolean;
  free_bytes: number;
  total_bytes: number;
  /** 数据目录在不在这块盘上 */
  holds_data: boolean;
}

/** 一个目标的现状。 */
export interface BackupTargetStatus {
  path: string;
  volume_label: string;
  /** 最近一次成功的本地日期；从没成功过是 null */
  last_success: string | null;
  /** 最近一次没成的原话（成功过就不再提旧的） */
  last_problem: string | null;
  /** 这块盘现在在不在（**按卷序列号认**；不在才是真的"没插/拔了"） */
  volume_present: boolean;
  /** 目标目录建了没——**没建是常态**（第一次备份会自动创建），不该当成故障 */
  dir_exists: boolean;
  /** 最近 7 天里缺了哪几天 */
  gaps: string[];
}

/** 备份设置页 / 提示条要的一整份现状。 */
export interface BackupStatus {
  config: BackupConfig;
  volumes: BackupVolume[];
  data_volume_id: string;
  has_other_volume: boolean;
  should_nudge: boolean;
  targets: BackupTargetStatus[];
  today: string;
}

/** 一个目标的备份结果。 */
export interface BackupTargetOutcome {
  path: string;
  volume_label: string;
  /** written / skipped / failed */
  status: string;
  reason: string;
  package: string;
  bytes: number;
  kept: number;
  removed: number;
  fingerprint: string;
  at: number;
}

/** 一次备份的总账。 */
export interface BackupReport {
  at: number;
  stamp: string;
  outcomes: BackupTargetOutcome[];
}

/** 一份备份包的摘要（恢复页列表用；只看清单，不做体检）。 */
export interface BackupPackage {
  path: string;
  stamp: string;
  created_at: number;
  device: string;
  works: number;
  chapters: number;
  words: number;
  bytes: number;
  data_format_version: number;
}

/** 能恢复的来源：看得见的备份包 + 扫过的位置 + 留底目录。 */
export interface RestoreSources {
  packages: BackupPackage[];
  roots: string[];
  data_dir: string;
  keep_dir: string;
}

/** 恢复前的体检结论 + "会退回多少"。 */
export interface RestorePreview {
  source: string;
  /** package（备份包）/ database（单个库文件） */
  kind: string;
  verify: { ok: boolean; problems: string[] };
  created_at: number;
  device: string;
  engine_version: string;
  data_format_version: number;
  source_last_write_at: number;
  source_works: number;
  source_chapters: number;
  source_words: number;
  source_bytes: number;
  live_last_write_at: number;
  live_works: number;
  live_words: number;
  /** 现在的库读得出来吗；读不出来时上面三个数没有意义，也就算不出"会丢多少" */
  live_readable: boolean;
  lost_days: number;
  lost_words: number;
  /** 选中的正是现在用的那个库（不能拿它恢复它自己） */
  is_live_database: boolean;
  can_restore: boolean;
  keep_dir: string;
}

/** 换库结果。 */
export interface RestoreOutcome {
  restored_from: string;
  /** 原库留底目录（原来就没有库时是空串） */
  quarantine: string;
  bytes: number;
  stamp: string;
}

/** 同级里与它重名的那一个（恢复前会摆给作者看）。 */
export interface NameClash {
  id: number;
  title: string;
  word_count: number;
  char_count: number;
  chars_no_punct: number;
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

/** 一条章节版本快照的摘要（**正文不在这里**，要比对时才拉）。 */
export interface SnapshotSummary {
  id: number;
  char_count: number;
  /** close / stuck / desync / keep / before_restore——界面查字典渲染 */
  reason: string;
  /** 作者亲手留的版本（不参与滚动删除） */
  pinned: boolean;
  created_at: number;
}

/** 差异里的一行。 */
export interface SnapshotDiffLine {
  /** same / added / removed / skipped */
  kind: string;
  text: string;
  /** 在快照那一版里的行号；新增行为 null */
  old_line: number | null;
  /** 在当前稿里的行号；删除行为 null */
  new_line: number | null;
  /** 折叠掉的相同行数（只有 skipped 非 0） */
  hidden: number;
}

/** 某条快照与**当前正文**的比对结果。 */
export interface SnapshotDiff {
  lines: SnapshotDiffLine[];
  added: number;
  removed: number;
  /** 块太大，没有逐行对齐（界面要说清"只给到这一层"） */
  truncated: boolean;
  current_char_count: number;
}

/** 一次回滚的回执：回滚后的正文（界面据此换掉编辑器内容并重建落盘基准）。 */
export interface SnapshotRestoreAck {
  node_id: number;
  body: string;
  char_count: number;
  chars_no_punct: number;
  word_count: number;
  fingerprint: string;
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
  chars_no_punct: number;
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

/** 稿子放在哪、要不要首启引导、推荐放哪（都是壳的只读报告）。 */
export interface LocationInfo {
  /** 当前数据目录 */
  path: string;
  /** 位置记录文件（"你凭什么记得稿子在哪"的答案） */
  pointer: string;
  /** 第一次用：界面该弹首启引导 */
  first_run: boolean;
  /** 推荐位置（首启时与 path 相同） */
  suggested_path: string | null;
  /** 推荐它的理由码：portable / documents / documents_synced / profile */
  suggested_reason: string | null;
}

/** 刚选好的新位置：落在哪、有什么坑、那儿现在有多少东西。 */
export interface PickedDir {
  path: string;
  /** 风险码：synced / desktop / drive_root / inside_data / occupied / removable */
  risks: string[];
  entries: number;
}

/** 搬完的账（旧位置一个字没删）。 */
export interface RelocationReport {
  path: string;
  files: number;
  bytes: number;
}


/** 命令白名单：新增命令先在这里登记，别在组件里裸调。 */
const COMMANDS = {
  engineInfo: "engine_info",
  dataHome: "data_home",
  openDataDir: "open_data_dir",
  diagnoseNote: "diagnose_note",
  locationInfo: "data_location_info",
  locationConfirm: "data_location_confirm",
  locationPick: "data_location_pick",
  locationMove: "data_location_move",
  locationCancel: "data_location_cancel",
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
  typesetRules: "typeset_rules",
  typesetScan: "typeset_scan",
  typesetApply: "typeset_apply",
  snapshotList: "snapshot_list",
  snapshotDiff: "snapshot_diff",
  snapshotKeep: "snapshot_keep",
  snapshotDrop: "snapshot_drop",
  snapshotRestore: "snapshot_restore",
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
  setWorkLanguage: "set_work_language",
  backupStatus: "backup_status",
  backupConfigWrite: "backup_config_write",
  backupNow: "backup_now",
  backupRestoreSources: "backup_restore_sources",
  backupRestorePreview: "backup_restore_preview",
  backupRestoreApply: "backup_restore_apply",
  backupRestorePick: "backup_restore_pick",
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
    // 参数先修一遍「半个字符」：JS 允许落单代理项，Rust 的 JSON 解析器直接拒收，
    // 真让它过去，用户会收到一句英文解析错误（见 api/errors.ts 的说明）。
    return await invoke<T>(command, args ? healText(args) : args);
  } catch (e) {
    throw asError(e);
  }
}

export const readEngineInfo = () => call<EngineInfo>(COMMANDS.engineInfo);
export const readDataHome = () => call<DataHome>(COMMANDS.dataHome);

/** 在文件管理器里打开稿子所在的目录（壳自己打开自己的目录，界面不传路径）。 */
export const openDataDir = () => call<void>(COMMANDS.openDataDir);

/**
 * 启动诊断：把界面上观察到的焦点/输入法事件递给壳。
 *
 * 壳没开 `--diagnose` 时这条命令是空转——所以界面这边不做开关判断，少一处"诊断没生效"的可能。
 */
export const diagnoseNote = (text: string) => call<void>(COMMANDS.diagnoseNote, { text });

/** 稿子现在放哪、要不要首启引导、推荐放哪。 */
export const readLocationInfo = () => call<LocationInfo>(COMMANDS.locationInfo);

/** 首启选「就用这里」：把当前位置记下来，之后不再弹引导。 */
export const confirmLocation = () => call<void>(COMMANDS.locationConfirm);

/**
 * 让作者挑一个新位置（窗口标题由界面给，壳不产文案）。
 *
 * **返回的只是一个"选到哪了"的报告**：那条路径同时被壳记在它自己手里，
 * 界面拿它去搬家是没有入口的——这是刻意的（见 `commands::location`）。
 */
export const pickDataDir = (title: string) =>
  call<PickedDir | null>(COMMANDS.locationPick, { title });

/** 搬到刚才选的位置（复制 + 核对 + 记下新位置；旧位置不删；成功后壳会重启）。 */
export const moveDataDir = () => call<RelocationReport>(COMMANDS.locationMove);

/** 放弃这次选择。 */
export const cancelDataDir = () => call<void>(COMMANDS.locationCancel);

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

/** 备份现状：配置 + 能看到的盘 + 每个目标的账本摘要与缺口。 */
export const readBackupStatus = (tz_offset_minutes: number) =>
  call<BackupStatus>(COMMANDS.backupStatus, { tz_offset_minutes });

/** 写备份偏好（写完回读）。 */
export const writeBackupConfig = (config: BackupConfig) =>
  call<BackupConfig>(COMMANDS.backupConfigWrite, { config });

/** 立即备份到所有目标（逐目标成败，一个盘写不进去不影响别的盘）。 */
export const runBackupNow = (tz_offset_minutes: number) =>
  call<BackupReport>(COMMANDS.backupNow, { tz_offset_minutes });

/** 能恢复的备份包（扫作者勾过的每个备份位置；盘不在就扫不到）。 */
export const readRestoreSources = () => call<RestoreSources>(COMMANDS.backupRestoreSources);

/** 恢复前看一眼：体检结论 + 会退回几天 / 少多少字。**不动任何文件。** */
export const previewRestoreSource = (source: string) =>
  call<RestorePreview>(COMMANDS.backupRestorePreview, { source });

/** 真正换库（先关库、原库留底；成功后壳会重启）。 */
export const applyRestore = (source: string, tz_offset_minutes: number) =>
  call<RestoreOutcome>(COMMANDS.backupRestoreApply, { source, tz_offset_minutes });

/** 让作者从磁盘上挑一个库文件（标题与筛选项由界面给，壳不产文案）。 */
export const pickRestoreDatabase = (title: string, filter_label: string) =>
  call<string | null>(COMMANDS.backupRestorePick, { title, filter_label });

/** 改作品语言的回执：改完的语言 + **落定后的字数口径**（界面照着刷新那个数字）。 */
export interface WorkLanguageAck {
  work_id: number;
  language: string;
  word_caliber: string;
}

/** 改作品语言（zh / en / ja）——字数默认口径跟它走；返回**写完回读**的两个码。 */
export const setWorkLanguage = (work_id: number, language: string) =>
  call<WorkLanguageAck>(COMMANDS.setWorkLanguage, { work_id, language });

/** 删掉一本书（软删除，正文与历史都留着）。 */
export const deleteWork = (work_id: number) => call<void>(COMMANDS.deleteWork, { work_id });

/** 这一章有哪些版本（**新的在前**，只有摘要，没有正文）。 */
export const snapshotList = (node_id: number) =>
  call<SnapshotSummary[]>(COMMANDS.snapshotList, { node_id });

/** 某条快照与**当前正文**差在哪（行级统一差异，结构化给界面画）。 */
export const snapshotDiff = (node_id: number, snapshot_id: number) =>
  call<SnapshotDiff>(COMMANDS.snapshotDiff, { node_id, snapshot_id });

/** 手动留一版；内容与最新一份相同就返回 null（不堆重复版本）。 */
export const snapshotKeep = (node_id: number) =>
  call<number | null>(COMMANDS.snapshotKeep, { node_id });

/** 删掉一条快照（**正文一个字都不动**）。 */
export const snapshotDrop = (snapshot_id: number) =>
  call<void>(COMMANDS.snapshotDrop, { snapshot_id });

/** 回滚到某一条快照——核心会**先替当前这一版留底**，再改正文。 */
export const snapshotRestore = (snapshot_id: number) =>
  call<SnapshotRestoreAck>(COMMANDS.snapshotRestore, { snapshot_id });

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

/** 排版规则清单（代码 + 风险档）——界面不自己抄一份"有哪些规则"。 */
export const typesetRules = () => call<TypesetRule[]>(COMMANDS.typesetRules);

/** 扫一遍排版（dry-run）：**只报告会怎么改，正文一个字不动**。 */
export const typesetScan = (text: string, options: TypesetOptions) =>
  call<TypesetReport>(COMMANDS.typesetScan, { text, options });

/** 应用勾中的那几处，返回改好的正文；序号对不上会整批拒绝（稿子又改过了）。 */
export const typesetApply = (text: string, options: TypesetOptions, accepted: number[]) =>
  call<string>(COMMANDS.typesetApply, { text, options, accepted });

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
