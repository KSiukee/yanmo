// 编译：投稿版 docx / 分章 txt / 合并 txt。

import { call, COMMANDS } from "./transport";

/** 一种编译预设（代码 + 核心给的默认参数；界面不自己编默认值）。 */
export interface CompilePreset {
  code: string;
  /** 这一种要不要卡正文上限（界面据此决定显不显示"前 N 字"） */
  caps_body: boolean;
  /** 默认上限（界面拿它当输入框初值） */
  default_body_limit: number;
  default_outline: boolean;
}

/** 一个将要生成的文件（界面只展示名字与大小）。 */
export interface CompileFile {
  /** 相对导出目录的路径（含预设子目录） */
  path: string;
  bytes: number;
}

/** 一次编译的回执。 */
export interface CompileAck {
  path: string;
  files: CompileFile[];
  /** 顺手清掉了几个上次留下的旧产物 */
  removed: number;
}

/** 编译预设清单（代码 + 核心给的默认参数）。 */
export const compilePresets = () => call<CompilePreset[]>(COMMANDS.compilePresets);

/** 会生成哪些文件（**不落盘**）：按"编译"之前先看清。 */
export const compilePreview = (
  work_id: number,
  preset: string,
  body_limit: number | null,
  with_outline: boolean,
  with_sources: boolean,
) =>
  call<CompileFile[]>(COMMANDS.compilePreview, {
    work_id,
    preset,
    body_limit,
    with_outline,
    with_sources,
  });

/** 真编译：渲染 + 落盘到导出目录里这一种预设自己的子目录。 */
export const compileWork = (
  work_id: number,
  preset: string,
  body_limit: number | null,
  with_outline: boolean,
  with_sources: boolean,
) =>
  call<CompileAck>(COMMANDS.compileWork, {
    work_id,
    preset,
    body_limit,
    with_outline,
    with_sources,
  });

/** 打开某种预设的产物目录（路径由壳自己算，界面不传路径）。 */
export const compileOpenFolder = (work_id: number, preset: string) =>
  call<void>(COMMANDS.compileOpenFolder, { work_id, preset });
