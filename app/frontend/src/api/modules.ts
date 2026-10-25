// 模块管理页的取值：**卡片 + 四个动作**。规则全在核心；这里只做名字映射。
//
// 纪律：界面不传路径、不碰文件；模块数据在哪由核心决定（卸载默认保留，删数据是另一个
// 显式动作，这一版还没开）。

import { call, COMMANDS } from "./core";

/** 管理页一行（与核心 `ModuleCard` 一一对应）。 */
export interface ModuleCard {
  id: string;
  name: string;
  description: string;
  version: string;
  author: string;
  certified: boolean;
  cert_edition: number;
  certified_at: number;
  enabled: boolean;
  /** 离开管理页后是否允许继续跑（不是自启） */
  background: boolean;
  favorite: boolean;
  pinned: boolean;
  reads: string[];
  own_data: boolean;
  network: boolean;
  network_declared: boolean;
  ui_form: string;
  fingerprint: string;
  source: string;
  installed_at: number;
  updated_at: number;
}

/** 模块声明的能力（与核心 `ModuleCapabilities` 一一对应；授予是它的子集）。 */
export interface ModuleCapabilities {
  read: string[];
  own_data: boolean;
  cross_read: string[];
  ask_questions: boolean;
  network: boolean;
}

/** 导入前的预览：**只给事实**，界面逐项征得同意。 */
export interface ImportPreview {
  module_id: string;
  name: string;
  description: string;
  version: string;
  author: string;
  protocol: number;
  expected_protocol: number;
  compatible: boolean;
  fingerprint: string;
  declared: ModuleCapabilities;
  entry_bytes: number;
  certified: boolean;
  cert_edition: number;
  certified_at: number;
}

export const modulesList = () => call<ModuleCard[]>(COMMANDS.modulesList);

export const moduleSetEnabled = (module_id: string, enabled: boolean) =>
  call<void>(COMMANDS.moduleSetEnabled, { module_id, enabled });

export const moduleSetFavorite = (module_id: string, favorite: boolean) =>
  call<void>(COMMANDS.moduleSetFavorite, { module_id, favorite });

export const moduleSetPinned = (module_id: string, pinned: boolean) =>
  call<void>(COMMANDS.moduleSetPinned, { module_id, pinned });

export const moduleSetBackground = (module_id: string, background: boolean) =>
  call<ModuleCard>(COMMANDS.moduleSetBackground, { module_id, background });

export const moduleSetNetwork = (module_id: string, network: boolean) =>
  call<ModuleCard>(COMMANDS.moduleSetNetwork, { module_id, network });

export const networkOffline = () => call<boolean>(COMMANDS.networkOffline, {});
export const networkAllowlist = () =>
  call<string[]>(COMMANDS.networkAllowlist, {});
export const networkSetAllowlist = (hosts: string[]) =>
  call<string[]>(COMMANDS.networkSetAllowlist, { hosts });
export const networkSetOffline = (offline: boolean) =>
  call<boolean>(COMMANDS.networkSetOffline, { offline });

export const moduleUninstall = (module_id: string) =>
  call<void>(COMMANDS.moduleUninstall, { module_id });

export const moduleImportPick = (title: string) =>
  call<ImportPreview | null>(COMMANDS.moduleImportPick, { title });

export const moduleImportConfirm = (granted: ModuleCapabilities, expected_fingerprint: string) =>
  call<ModuleCard>(COMMANDS.moduleImportConfirm, { granted, expected_fingerprint });

export const moduleImportCancel = () => call<void>(COMMANDS.moduleImportCancel);

/** 启动回执；真正结果用 `moduleStatus` 取。 */
export interface ModuleRunStarted {
  module_id: string;
  /** 启动前那份镜像：`fresh`（本来就一致）/ `written`（这次写了）/ `failed`。 */
  mirror_state: string;
  mirror_error: string | null;
}

/** 一次运行的状态快照。 */
export interface ModuleStatus {
  known: boolean;
  running: boolean;
  stopped: boolean;
  timed_out: boolean;
  idle_seconds: number;
  exit_code: number | null;
  emitted: string[];
  stderr: string;
  error: string | null;
  data_degraded: boolean;
  data_rejected: number;
}

export const moduleRun = (module_id: string) =>
  call<ModuleRunStarted>(COMMANDS.moduleRun, { module_id });

export const moduleStatus = (module_id: string) =>
  call<ModuleStatus>(COMMANDS.moduleStatus, { module_id });

export const moduleStop = (module_id: string) =>
  call<void>(COMMANDS.moduleStop, { module_id });

export interface CredentialBinding {
  name: string;
  host: string;
}

export const credentialsList = (module_id: string) =>
  call<CredentialBinding[]>(COMMANDS.credentialsList, { module_id });

export const credentialAdd = (
  module_id: string,
  name: string,
  host: string,
  secret: string,
) => call<CredentialBinding>(COMMANDS.credentialAdd, { module_id, name, host, secret });

export const credentialRemove = (module_id: string, name: string) =>
  call<void>(COMMANDS.credentialRemove, { module_id, name });

export interface NetworkAuditRow {
  ts: number;
  module: string;
  host: string;
  bytes: number;
  credential: string | null;
  result: string;
}

export const networkAuditRecent = (
  limit = 20,
  module: string | null = null,
  result: string | null = null,
) =>
  call<NetworkAuditRow[]>(COMMANDS.networkAuditRecent, {
    limit,
    module,
    result,
  });

export const networkAuditExport = (
  module: string | null = null,
  result: string | null = null,
) =>
  call<string>(COMMANDS.networkAuditExport, { module, result });

export const moduleUiOpen = (module_id: string) =>
  call<void>(COMMANDS.moduleUiOpen, { module_id });

export const moduleDataExport = (module_id: string, title: string) =>
  call<boolean>(COMMANDS.moduleDataExport, { module_id, title });

export const moduleDataClear = (module_id: string) =>
  call<number>(COMMANDS.moduleDataClear, { module_id });

export interface ModuleDataStats {
  tables: number;
  rows: number;
  db_bytes: number;
  degraded: boolean;
  rejected: number;
  isolated: boolean;
  isolation_reason: string;
}

export const moduleDataStats = (module_id: string) =>
  call<ModuleDataStats>(COMMANDS.moduleDataStats, { module_id });

/** 体检里的一条事实：code 是稳定码，detail 是技术细节（不当界面句子用）。 */
export interface ModuleDataIssue {
  code: string;
  detail: string;
}

/** 体检报告：只给事实 + 一句结论（ok / attention / damaged），没有评分。 */
export interface ModuleDataAudit {
  module_id: string;
  integrity: string;
  integrity_raw: string;
  tables: number;
  rows: number;
  stray_tables: number;
  isolated: boolean;
  isolation_reason: string;
  degraded: boolean;
  issues: ModuleDataIssue[];
  verdict: string;
}

export const moduleDataAudit = (module_id: string) =>
  call<ModuleDataAudit>(COMMANDS.moduleDataAudit, { module_id });

export const moduleDataIsolate = (module_id: string, reason: string) =>
  call<ModuleIsolateReceipt>(COMMANDS.moduleDataIsolate, { module_id, reason });

export const moduleDataRelease = (module_id: string) =>
  call<boolean>(COMMANDS.moduleDataRelease, { module_id });

/** 一份开放格式镜像的回执（落在 <数据目录>/module-mirrors/<模块 id>.json）。 */
export interface ModuleMirror {
  module_id: string;
  path: string;
  bytes: number;
  tables: number;
  rows: number;
  written_at: number;
}

/** 镜像与主库的一处不一致：code 是稳定码，detail 是表名。 */
export interface MirrorDifference {
  code: string;
  detail: string;
}

export interface ModuleMirrorCheck {
  module_id: string;
  /** `ok` / `different` / `missing` */
  state: string;
  written_at: number;
  tables: number;
  rows: number;
  differences: MirrorDifference[];
}

export interface ModuleMirrorRefresh {
  written: ModuleMirror;
  /** 写之前的旧镜像核对结果（以前没写过 = null）。 */
  before: ModuleMirrorCheck | null;
  /** 旧镜像读不出来时的原因。 */
  before_error: string | null;
}

export interface ModuleIsolateReceipt {
  isolated: boolean;
  mirrored: boolean;
  mirror_error: string | null;
}

export interface ModuleRebuild {
  module_id: string;
  tables: number;
  rows: number;
  schema_version: number | null;
}

export const moduleDataMirror = (module_id: string) =>
  call<ModuleMirrorRefresh>(COMMANDS.moduleDataMirror, { module_id });

/** 「全部留镜像」的回执。 */
export interface MirrorAllReceipt {
  modules: number;
  tables: number;
  rows: number;
}

export const moduleDataMirrorAll = () =>
  call<MirrorAllReceipt>(COMMANDS.moduleDataMirrorAll);

export const moduleDataRebuild = (module_id: string) =>
  call<ModuleRebuild>(COMMANDS.moduleDataRebuild, { module_id });
