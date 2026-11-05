// 模块 API 的类型面：只放 interface，不放命令。
// 拆出来是因为命令文件（modules.ts）靠类型之外的东西本来就能读懂；
// 混在一起它会一直长到「上帝文件」线上。

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
  /** 已授予「把正文交给核心」 */
  deliver_body: boolean;
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
  /** 可否把正文交给核心（默认关） */
  deliver_body: boolean;
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
  /** 库里已经装着同一个 id（升级 / 同版 / 降级三种情形都从这里看）。 */
  installed: boolean;
  installed_version: string;
  /** 装着的那个版本的说明（跟新说明比，有变化就说「变更说明」）。 */
  installed_description: string;
  /** 新版本严格更高 → 可以升级。 */
  upgrade: boolean;
  /** 新版本不比装着的版本新 → 按纪律拒。 */
  downgrade: boolean;
  /** 现在授予的能力（升级时用它预勾选）。 */
  granted: ModuleCapabilities;
  /** 这次会新增的能力码（空 = 能力没扩大）。 */
  added: string[];
}
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
export interface CredentialBinding {
  name: string;
  host: string;
}
export interface NetworkAuditRow {
  ts: number;
  module: string;
  host: string;
  bytes: number;
  credential: string | null;
  result: string;
}
export interface ModuleDataStats {
  tables: number;
  rows: number;
  db_bytes: number;
  degraded: boolean;
  rejected: number;
  isolated: boolean;
  isolation_reason: string;
}
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
/** 「全部留镜像」的回执。 */
export interface MirrorAllReceipt {
  modules: number;
  tables: number;
  rows: number;
}
/** 核心观测到的这一格的数据事实（透明度面板的第一组）。 */
export interface ModuleFacts {
  module_id: string;
  tables: number;
  rows: number;
  data_bytes: number;
  data_bytes_partial: boolean;
  db_bytes: number;
  schema_version: number | null;
  degraded: boolean;
  rejected: number;
  isolated: boolean;
  isolation_reason: string;
  mirror_written_at: number;
  last_run_at: number;
  runs: number;
}
/** 出网日志的汇总（壳观测；只覆盖日志窗口里有的记录）。 */
export interface NetworkSummary {
  scanned: number;
  requests: number;
  ok: number;
  failed: number;
  bytes: number;
  last_at: number;
  last_host: string;
  last_result: string;
}
/** 读账里的一本书（最近 24 小时）。 */
export interface ReadBookView {
  work_id: number;
  title: string | null;
  reads: number;
  chars: number;
}
/** 一个模块最近 24 小时的读账小结（核心观测）。 */
export interface ReadSummaryView {
  reads: number;
  denied: number;
  chars: number;
  books: ReadBookView[];
}
/** 事实页的整份视图：注册表那一行 + 核心观测 + 壳观测。 */
export interface ModuleFactsView {
  card: ModuleCard;
  facts: ModuleFacts;
  program_bytes: number;
  network: NetworkSummary;
  reads: ReadSummaryView;
  archive: ArchiveView | null;
  cert: CertView | null;
  runs: RunEventView[];
  /** 退避账：连续失败几次、自动唤起要等到什么时候（没失败过 = null）。 */
  backoff: { failures: number; next_at: number } | null;
  running: boolean;
  idle_seconds: number;
}
/** 一次唤起的留痕（链路可见的最小单位）。 */
export interface RunEventView {
  trigger: string;
  started_at: number;
  duration_ms: number;
  outcome: string;
  code: string;
}
/** 认证：验过什么（版本号决定清单；认不出的版本 checks 为空）。 */
export interface CertView {
  edition: number;
  date: string;
  checks: string[];
}
/** 上一版留档（有才能回滚）。 */
export interface ArchiveView {
  version: string;
  archived_at: number;
}
export interface RollbackReceipt {
  module_id: string;
  from_version: string;
  to_version: string;
}
/** 一件等作者回答的读取放行请求。 */
export interface ReadAskView {
  module_id: string;
  /** `first`（只告知）/ `second`（问要不要放宽）/ `ultimate`（第三次：不再干预）。 */
  kind: string;
  refusals: number;
  last_work: string | null;
}
