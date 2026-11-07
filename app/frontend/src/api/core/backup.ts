// 备份、恢复与创作留痕包。

import { call, COMMANDS } from "./transport";

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
  /** 备份里带不带模块数据（默认带；关掉后包里只有稿子，恢复时也不动现有模块数据） */
  include_modules: boolean;
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
  /** 作者开了加密（写出去的包要口令才打得开） */
  sealed: boolean;
  /** 此刻内存里有没有口令——**重启之后是 false**，界面要如实说 */
  seal_unlocked: boolean;
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
  /** 账本（「最后一次成功 / 缺了哪几天」靠它）写成功了吗；false ＝ 包写好了但账没记上 */
  ledger_written: boolean;
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
  /** 加密包（`.ympkg`）：清单在密文里——上面那几栏全是 0 与空串，只有 stamp / created_at / bytes 是真的 */
  sealed: boolean;
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
  /** 这份备份带了模块数据、并且已经一起换上去；false = 现有模块数据保持原样 */
  modules_restored: boolean;
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

/** 备份现状：配置 + 能看到的盘 + 每个目标的账本摘要与缺口。 */
export const readBackupStatus = (tz_offset_minutes: number) =>
  call<BackupStatus>(COMMANDS.backupStatus, { tz_offset_minutes });

/** 写备份偏好（写完回读）。 */
export const writeBackupConfig = (config: BackupConfig) =>
  call<BackupConfig>(COMMANDS.backupConfigWrite, { config });

/** 立即备份到所有目标（逐目标成败，一个盘写不进去不影响别的盘）。 */
/** 做一次备份。`manual`：作者亲手点的「立即备份」传 true，机器自己按日做的那两次传 false——
 *  手动那份包名带记号，核心的滚动清理**永不删它**（自动的才按份数轮转）。 */
export const runBackupNow = (tz_offset_minutes: number, manual: boolean) =>
  call<BackupReport>(COMMANDS.backupNow, { tz_offset_minutes, manual });

/** 创作留痕包：作者挑一个文件夹，壳把核心打好的几个文件写进去；取消返回 null。 */
export interface TraceExport {
  dir: string;
  files: number;
  events: number;
  head: string;
}

export const exportTracePackage = (
  work_id: number,
  tz_offset_minutes: number,
  title: string,
  scope: "all" | "creative" = "all",
) => call<TraceExport | null>(COMMANDS.traceExport, { work_id, tz_offset_minutes, title, scope });

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
