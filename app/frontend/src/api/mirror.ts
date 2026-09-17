// 磁盘 `.md` 镜像那几条命令的**类型与调用**（命令名仍在 `./core` 的白名单里统一登记）。
//
// 为什么单开一个文件：`core.ts` 是"唯一能 import Tauri API 的入口"，它已经在上限边上了
// （仓库里有条行数棘轮：已登记的超限文件只许减不许增）。新域的口径同 `volumes.ts`：
// **命令名进白名单（`core.ts`），类型与薄封装放各自域的文件里**——通道仍然只有一条。

import { call, COMMANDS } from "./core";

/**
 * 磁盘 `.md` 镜像现在什么样（壳里那份对账报告的只读快照）。
 *
 * `conflicts` 是**有人在我们之外改过、暂未覆盖**的文件数：研墨绝不静默盖掉作者改过的东西，
 * 所以这些文件停在原样、等作者定夺（合并 / 覆盖）。`last_error` 是技术说明，只放进日志味的提示里。
 */
export interface MirrorStatus {
  enabled: boolean;
  root: string;
  /** 上次对完账的时间（毫秒；0 = 还没对过） */
  last_sync_at: number;
  files: number;
  /** 外面改过、还没定夺的文件数 */
  conflicts: number;
  /** 镜像目录里没账的 `.md` 数（研墨不会动它们） */
  untracked: number;
  /** 要作者定夺的事的明细（可能被截断；计数以上面两个为准） */
  issues: MirrorIssue[];
  failed_works: number;
  last_error: string;
}

/**
 * 一件要作者定夺的事：磁盘上那一份与研墨这一边不一样了。
 *
 * `kind` 三种：`edited`（我们写过的那份被别的工具改过）、`foreign`（想写的路径上有别人的东西）、
 * `untracked`（镜像目录里没账的 `.md`，多半是作者自己挪过名字）——前者能动，后者只能看。
 */
export interface MirrorIssue {
  node_id: number;
  title: string;
  relative_path: string;
  kind: "edited" | "foreign" | "untracked" | string;
}

/** 读一眼状态（读的是壳里那份内存报告，很便宜）。 */
export const readMirrorStatus = () => call<MirrorStatus>(COMMANDS.mirrorStatus);

/** 开 / 关镜像（关掉**不删**磁盘上已有的 `.md`）。 */
export const setMirrorEnabled = (enabled: boolean) =>
  call<MirrorStatus>(COMMANDS.mirrorSetEnabled, { enabled });

/** 请镜像立刻做一次全量核对（异步：信号送到就返回，状态隔一会儿再看）。 */
export const syncMirrorNow = () => call<void>(COMMANDS.mirrorSyncNow);

/** 在文件管理器里打开镜像目录。 */
export const openMirrorDir = () => call<void>(COMMANDS.mirrorOpenFolder);

/**
 * 处置一条待定夺的事。
 *
 * **不传路径**：只传"第几条"（清单是壳产出来的）+ 那个节点 id 做核对——界面从头到尾
 * 拿不到、也递不进一个可操作的文件路径（与"命令不接受路径参数"同一条纪律）。
 */
export const resolveMirrorIssue = (index: number, node_id: number, action: "adopt" | "overwrite") =>
  call<MirrorStatus>(COMMANDS.mirrorResolve, { index, node_id, action });
