// 编辑器那一族：打开/落盘/光标/会话/关窗/检索。

import { call, COMMANDS, listen, type UnlistenFn } from "./transport";
import type { SaveOrigin, SourceBreakdown } from "../source";

/** 打开编辑器时拿到的一章。 */
export interface EditorSnapshot {
  work_id: number;
  node_id: number;
  title: string;
  /** 这一章的"一句话"（作者手填，空串＝没写过）：投稿包的大纲要用它 */
  summary: string;
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
  /** 打开的是场景卡时的那四格；别的节点是 null */
  scene: SceneFields | null;
  /** 这一章的字从哪来（老稿子是一档「未记录」） */
  sources: SourceBreakdown;
}

/** 场景卡的四格（**只对场景卡有**：别的节点读出来是 null）。 */
export interface SceneFields {
  node_id: number;
  /** 视角：这一场从谁眼里看出去 */
  pov: string;
  /** 目标：这一场想要什么 */
  goal: string;
  /** 冲突：挡在目标前面的是什么 */
  conflict: string;
  /** 结果：这一场结束时落到哪儿 */
  outcome: string;
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

/** 一次落盘的回执。 */
export interface SaveAck {
  char_count: number;
  chars_no_punct: number;
  word_count: number;
  /** 落盘后库里的内容指纹（写后读回校验的比对基准） */
  fingerprint: string;
  /** 这一章现在的来源分布（与字数同一笔账的另一面） */
  sources: SourceBreakdown;
}

/** 上一次会话的交代。 */
export interface SessionNotice {
  /** 上次没有正常退出（被杀 / 崩溃） */
  unclean: boolean;
  last_node_id: number | null;
  /** 上次活动时间（unix 毫秒） */
  last_seen_at: number | null;
  /** 上次会话里界面卡过没有（带着现场：第几次、卡在哪一章、最后一版指纹） */
  revive: { at: number; node_id: number | null; fingerprint: string; attempt: number; gave_up: boolean } | null;
  /** 守护之心的现状（`alive` / `probing` / `waiting` / `gave_up`）与本次会话重载次数 */
  watchdog: string;
  revives: number;
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

/** 一条正文检索的命中。 */
export interface SearchHit {
  /** 这一章 / 这一篇的节点 id（点它跳过去） */
  node_id: number;
  work_id: number;
  /** 书名；作者还没给书起名时是空串（界面给占位，核心不替他编名字） */
  work_title: string;
  /** 章名 / 篇名（命中在标题里时 `matched_title` 为真，那一段要标出来） */
  title: string;
  /** 命中周围的片段（**不带高亮标记**，高亮由界面自己算）；只在标题里命中时是空串 */
  snippet: string;
  matched_title: boolean;
}

/** 在正文里找一句话：`work_id` 传 null = 所有书；`limit` 传 0 = 核心默认上限（50）。 */
export const searchText = (query: string, work_id: number | null, limit: number) =>
  call<SearchHit[]>(COMMANDS.searchText, { query, work_id, limit });

/** 当前该编辑的一章（首次运行会引导出一篇默认作品；上次被杀则回到崩前那一章）。 */
export const openEditorTarget = () => call<EditorSnapshot>(COMMANDS.openEditorTarget);

/** 切到指定章节（这一章必须真的能编辑，否则核心会明确报错）。 */
export const openChapter = (node_id: number) =>
  call<EditorSnapshot>(COMMANDS.openChapter, { node_id });

/** 切到某一本书：落点是它上次写的那一章（记不起来就是第一章）。 */
export const openWorkTarget = (work_id: number) =>
  call<EditorSnapshot>(COMMANDS.openWorkTarget, { work_id });

/** 在当前章后面新建一章（目录树接上前的最小入口）。 */
export const createChapter = (node_id: number, title: string, kind = "chapter") =>
  call<EditorSnapshot>(COMMANDS.createChapter, { node_id, title, kind });

/** 上一章 / 下一章（按阅读顺序，跨卷）。 */
export const chapterNeighbors = (node_id: number) =>
  call<ChapterNeighbors>(COMMANDS.chapterNeighbors, { node_id });

/** 记下"这一章读到哪了"（失焦 / 切章 / 关窗时调用）。 */
export const saveCursor = (node_id: number, cursor: EditorCursor) =>
  call<void>(COMMANDS.saveCursor, { node_id, ...cursor });

/**
 * 落盘正文（防抖后调用；内容没变时核心不写库）。
 *
 * `tz_offset_minutes` 是本地时区相对 UTC 的偏移（东八区 = 480，见 `editor/backup.ts`
 * 的 `localOffsetMinutes`）：核心拿它算"这一笔算哪一天"，账本在 `writing_days`。
 * `origin` 是壳观测到的**这一笔的来源**（键入 / 粘贴 / 混合…）：核心只记它说的，
 * 没给就如实记「未记录」。
 */
export const saveBody = (
  node_id: number,
  body: string,
  tz_offset_minutes: number,
  origin?: SaveOrigin,
) => call<SaveAck>(COMMANDS.saveBody, { node_id, body, tz_offset_minutes, origin: origin ?? null });

/** 写当前章的"一句话"（投稿包的大纲要用它）；正文一个字都不动。 */
export const setNodeSummary = (node_id: number, summary: string) =>
  call<void>(COMMANDS.setNodeSummary, { node_id, summary });

/**
 * 存一个节点的四格（视角 / 目标 / 冲突 / 结果）。
 *
 * 与"章纲一句话"同一条路：低频手填字段，不参与正文的落盘防抖与指纹校验，
 * 也不动正文一个字节。**凡承载正文的节点都有这四格**（章也能直接填），
 * 卷没有——那种会被核心当场拒（`node.no_fields`）。
 */
export const saveNodeFields = (fields: SceneFields) =>
  call<SceneFields>(COMMANDS.saveNodeFields, { ...fields });

/** 库里正文的指纹——读回校验用，不搬运正文。 */
export const bodyFingerprint = (node_id: number) =>
  call<string>(COMMANDS.bodyFingerprint, { node_id });

/** 抢救：先把手上这份留成快照，再把库改回这一版（抢救回来的字也记进今日进度）。 */
export const emergencySnapshot = (
  node_id: number,
  body: string,
  reason: string,
  tz_offset_minutes: number,
  origin?: SaveOrigin,
) =>
  call<SaveAck>(COMMANDS.emergencySnapshot, {
    node_id,
    body,
    reason,
    tz_offset_minutes,
    origin: origin ?? null,
  });

/** 上次会话的交代（崩溃检测）。 */
export const sessionReport = () => call<SessionNotice>(COMMANDS.sessionReport);

/** 探活的回话：壳问"你还活着吗"，界面立刻回一声（守护之心的心跳之一）。 */
export const uiAlive = () => call<void>(COMMANDS.uiAlive);

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

/** 壳来探活：**立刻**回一声（回话即心跳）。页面死在 JS 死循环里时它永远排着不执行——回不了话正是判据。 */
export const onUiProbe = (handler: () => void): Promise<UnlistenFn> =>
  listen("ui-probe", () => handler());
