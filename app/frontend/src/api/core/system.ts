// 系统与位置：引擎信息、字体、数据目录、搬库、窗口与版本。

import { call, COMMANDS, getVersion, getCurrentWindow, asError } from "./transport";

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

export const readEngineInfo = () => call<EngineInfo>(COMMANDS.engineInfo);

/** 系统里装了的字体族（只读注册表；排版里「更多字体…」的可搜索列表用它）。 */
export const systemFonts = () => call<string[]>(COMMANDS.systemFonts);

/** 作者挑的一个字体文件：**只有名字与地址，没有路径**（壳复制进数据目录）。 */
export interface FontPick {
  name: string;
  url: string;
}

/** 让作者挑一个字体文件；取消返回 null。标题与过滤器文案由界面给（壳里不写中文）。 */
export const pickFont = (title: string, filter_label: string) =>
  call<FontPick | null>(COMMANDS.pickFont, { title, filter_label });

/** 重启之后把已存字体文件的地址找回来（文件不在了就是 null）。 */
export const fontUrl = (name: string) => call<string | null>(COMMANDS.fontUrl, { name });

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

/**
 * 全屏开关：**窗口能力，不走核心**（只改窗口，正文一个字都不动）。
 *
 * 干完**读回一次真实状态**再返回：全屏是窗口的真实状态（系统/作者都可能改动它），
 * 拿返回值当真相，界面那份 ref 就不会跟实际情况慢慢漂开。
 *
 * 权限要在 `capabilities/default.json` 里逐项列（`core:window:allow-set-fullscreen`）——
 * `core:window:default` 只给了"读"（is-fullscreen），没给"写"。
 */
export async function setFullscreen(on: boolean): Promise<boolean> {
  try {
    const appWindow = getCurrentWindow();
    await appWindow.setFullscreen(on);
    return await appWindow.isFullscreen();
  } catch (error) {
    throw asError(error);
  }
}

/** 现在是不是全屏（启动时对一次表；之后用 `setFullscreen` 的返回值维护）。 */
export async function isFullscreen(): Promise<boolean> {
  try {
    return await getCurrentWindow().isFullscreen();
  } catch (error) {
    throw asError(error);
  }
}

/**
 * **安装包版本**（作者在「应用和功能」里看到的那个号）。
 *
 * 与核心报的引擎版本是两件事：引擎版本由编译期注入、跟着 `yanmo-core` 走；
 * 安装包版本跟打包配置走。今天两者同号，混搭（换核心 / 单独驱动 CLI）时才会分开。
 */
export async function readAppVersion(): Promise<string> {
  try {
    return await getVersion();
  } catch (error) {
    throw asError(error);
  }
}
