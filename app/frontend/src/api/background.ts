// 正文背景图：**作者选的那张图由壳复制进数据目录**，界面只拿"文件名 + 一段 data URL"。
//
// 边界（与 `commands/background.rs` 一头一尾对着）：
// - **界面拿不到路径**：壳里选的、壳里复制的，回来的只有文件名与字节数——
//   所以这里也没有任何"路径"类型的参数，`pick` 只传窗口标题与筛选标签（壳不产文案）。
// - **渲染靠 data URL**：CSP 的 `img-src` 本来就允许 `data:`，所以不必开文件系统能力、
//   也不必为该目录注册自定义协议（安装包里也不会因此多一个字节）。
// - **图不在了不是错**：`present: false` 就是"回纯色"，界面有一句对应的人话。

import { call, COMMANDS } from "./core";

/** 换图 / 清图 / 读现状之后的回执。 */
export interface BackgroundOutcome {
  /** 现在用的是哪个文件（null = 没有图） */
  name: string | null;
  /** 渲染用的 `data:` URL；null = 现在没有图可画 */
  data_url: string | null;
  /** 这张图多少字节 */
  bytes: number | null;
  /** 这张图**还在**（被作者从数据目录里删掉 / 搬走时是 false） */
  present: boolean;
  /** 作者取消了选图——其余字段保持原样 */
  cancelled: boolean;
}

/** 现在的背景图（打开设置面板时问一次）。 */
export const backgroundImage = () => call<BackgroundOutcome>(COMMANDS.backgroundImage);

/** 让作者挑一张图（复制进数据目录并立刻生效；取消时 `cancelled: true`）。 */
export const backgroundPick = (title: string, filter_label: string) =>
  call<BackgroundOutcome>(COMMANDS.backgroundPick, { title, filter_label });

/** 清掉背景图（库里那一格清空 + 数据目录里那几张删掉）。 */
export const backgroundClear = () => call<BackgroundOutcome>(COMMANDS.backgroundClear);
