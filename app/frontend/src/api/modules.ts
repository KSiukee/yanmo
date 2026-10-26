// 模块管理页的取值：**卡片 + 四个动作**。规则全在核心；这里只做名字映射。
//
// 纪律：界面不传路径、不碰文件；模块数据在哪由核心决定（卸载默认保留，删数据是另一个
// 显式动作，这一版还没开）。

import type {
  CredentialBinding,
  ImportPreview,
  MirrorAllReceipt,
  ModuleCapabilities,
  ModuleCard,
  ModuleDataAudit,
  ModuleDataStats,
  ModuleFactsView,
  ModuleIsolateReceipt,
  ModuleMirrorRefresh,
  ModuleRebuild,
  ModuleRunStarted,
  ModuleStatus,
  NetworkAuditRow,
  ReadAskView,
  RollbackReceipt,
} from "./module-types";

// 类型面的家搬到 module-types.ts（这里只留命令）；老引用照旧能用。
export type * from "./module-types";

import { call, COMMANDS } from "./core";




export const modulesList = () => call<ModuleCard[]>(COMMANDS.modulesList);

/** 恐慌按钮：停用所有第三方模块（官方模块与稿子照常）。返回停用了几个。 */
export const modulesPanic = () => call<number>(COMMANDS.modulesPanic);

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



export const moduleRun = (module_id: string) =>
  call<ModuleRunStarted>(COMMANDS.moduleRun, { module_id });

export const moduleStatus = (module_id: string) =>
  call<ModuleStatus>(COMMANDS.moduleStatus, { module_id });

export const moduleStop = (module_id: string) =>
  call<void>(COMMANDS.moduleStop, { module_id });


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


export const moduleDataStats = (module_id: string) =>
  call<ModuleDataStats>(COMMANDS.moduleDataStats, { module_id });



export const moduleDataAudit = (module_id: string) =>
  call<ModuleDataAudit>(COMMANDS.moduleDataAudit, { module_id });

export const moduleDataIsolate = (module_id: string, reason: string) =>
  call<ModuleIsolateReceipt>(COMMANDS.moduleDataIsolate, { module_id, reason });

export const moduleDataRelease = (module_id: string) =>
  call<boolean>(COMMANDS.moduleDataRelease, { module_id });







export const moduleDataMirror = (module_id: string) =>
  call<ModuleMirrorRefresh>(COMMANDS.moduleDataMirror, { module_id });


export const moduleDataMirrorAll = () =>
  call<MirrorAllReceipt>(COMMANDS.moduleDataMirrorAll);






export const moduleFacts = (module_id: string) =>
  call<ModuleFactsView>(COMMANDS.moduleFacts, { module_id });




export const moduleRollback = (module_id: string) =>
  call<RollbackReceipt>(COMMANDS.moduleRollback, { module_id });

/** 半自动登记：模块从正文里找到的候选（还不是设定卡的名字）。 */
export interface ModuleCandidate {
  name: string;
  kind: string;
  chapters: number[];
  count: number;
}

export interface ModuleCandidatesView {
  work_id: number;
  items: ModuleCandidate[];
  existing: number;
}

export const moduleCandidates = (module_id: string) =>
  call<ModuleCandidatesView>(COMMANDS.moduleCandidates, { module_id });

export const moduleCandidatesApply = (module_id: string, names: string[]) =>
  call<number>(COMMANDS.moduleCandidatesApply, { module_id, names });

/** 把候选送进叩问通道（限频与去重由核心把关）。 */
/** 一条正文提案（模块提的，作者审的）。 */
export interface ProposalView {
  id: string;
  node_id: number;
  title: string;
  base_hash: string;
  /** 稿子已经变过 = 这条建议失效（不许静默套用）。 */
  stale: boolean;
  chars: number;
  /** `text` / `rename` / `move` */
  kind: string;
  parent: number | null;
  index: number;
  /** 成卷：要挪进新卷的节点（id + 当时的树指纹） */
  nodes: [number, string][];
  target: number | null;
  source: number | null;
  source_hash: string;
}

export interface ProposalsView {
  items: ProposalView[];
  /** 今日额度用完之后没有摆出来的条数（如实说，不许装作没有） */
  suppressed: number;
}

export interface ProposalOutcome {
  status: "applied" | "stale" | "missing";
  chars?: number;
  current_hash?: string;
}

export const moduleProposals = (module_id: string) =>
  call<ProposalsView>(COMMANDS.moduleProposals, { module_id });

export const moduleProposalApply = (module_id: string, proposal_id: string) =>
  call<ProposalOutcome>(COMMANDS.moduleProposalApply, { module_id, proposal_id });

export const moduleProposalDismiss = (module_id: string, proposal_id: string) =>
  call<void>(COMMANDS.moduleProposalDismiss, { module_id, proposal_id });

/** 词表里的一条（名字 + 分类）。 */
export interface WordRow {
  name: string;
  kind: string;
}

export const moduleWords = (module_id: string) =>
  call<{ items: WordRow[] }>(COMMANDS.moduleWords, { module_id });

export const moduleWordsWrite = (module_id: string, items: WordRow[]) =>
  call<number>(COMMANDS.moduleWordsWrite, { module_id, items });

/** 一条真实走过的链路（A → B）。 */
export interface ChainEdgeView {
  from: string;
  to: string;
  from_name: string;
  to_name: string;
  runs: number;
  last_at: number;
  failures: number;
  cut: boolean;
}

export const moduleEdges = () => call<ChainEdgeView[]>(COMMANDS.moduleEdges);

export const moduleEdgeCut = (from: string, to: string) =>
  call<void>(COMMANDS.moduleEdgeCut, { from, to });

export const moduleEdgeRestore = (from: string, to: string) =>
  call<boolean>(COMMANDS.moduleEdgeRestore, { from, to });

/** 管理页健康读数：每个模块一行。 */
export interface ModuleHealthView {
  module_id: string;
  last_at: number;
  last_outcome: string;
  last_code: string;
  failures: number;
  backoff_until: number;
  isolated: boolean;
}

export const moduleHealthList = () =>
  call<ModuleHealthView[]>(COMMANDS.moduleHealthList);

/** 每模块的退避参数（默认 10 秒起 / 5 分钟封顶）。 */
export interface RetryView {
  module_id: string;
  base_ms: number;
  max_ms: number;
  enabled: boolean;
}

export const moduleRetryGet = (module_id: string) =>
  call<RetryView>(COMMANDS.moduleRetryGet, { module_id });

export const moduleRetrySet = (
  module_id: string,
  base_ms: number,
  max_ms: number,
  enabled: boolean,
) => call<RetryView>(COMMANDS.moduleRetrySet, { module_id, base_ms, max_ms, enabled });

export const moduleRetryClear = (module_id: string) =>
  call<void>(COMMANDS.moduleRetryClear, { module_id });

/** 跑完补镜像：只给变过的模块写（返回写了哪几个）。 */
export const moduleMirrorChanged = () =>
  call<string[]>(COMMANDS.moduleMirrorChanged);

/** 整库恢复：一份镜像目录放回新库。 */
export interface RestoreItem {
  module_id: string;
  ok: boolean;
  tables: number;
  rows: number;
  detail: string;
}

export interface RestoreReport {
  found: number;
  items: RestoreItem[];
}

export const moduleRestorePick = (title: string) =>
  call<string | null>(COMMANDS.moduleRestorePick, { title });

export const moduleRestoreAll = (from: string) =>
  call<RestoreReport>(COMMANDS.moduleRestoreAll, { from });

export interface QuestionSubmitView {
  accepted: number;
  dropped: number;
  skipped: number;
  muted: boolean;
}

export const moduleQuestionsSubmit = (module_id: string) =>
  call<QuestionSubmitView>(COMMANDS.moduleQuestionsSubmit, { module_id });

/** 认证启动复核的结果：启动时按官方清单重核，变了什么就报什么。 */
export interface CertificationRecheckView {
  revoked: string[];
  restored: string[];
}

export const certificationsRecheck = () =>
  call<CertificationRecheckView>(COMMANDS.certificationsRecheck);


export const moduleReadAsks = () => call<ReadAskView[]>(COMMANDS.moduleReadAsks);

export const moduleReadAnswer = (module_id: string, answer: string) =>
  call<boolean>(COMMANDS.moduleReadAnswer, { module_id, answer });

export const moduleDataRebuild = (module_id: string) =>
  call<ModuleRebuild>(COMMANDS.moduleDataRebuild, { module_id });
