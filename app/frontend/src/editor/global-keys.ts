// 全局快捷键的**接线**：把键位表（`shortcuts.ts`）接到会话的具体动作上。
//
// 为什么单独一层：会话文件已经很长，而"按键到底认没认、会不会跟输入法打架"
// 恰恰是在这里决定的——给一个假窗口就能把整条链路测完（见 `global-keys.test.ts`）。

import {
  attachShortcuts,
  isTextFieldTarget,
  matchShortcut,
  type KeyTarget,
  type ShortcutId,
} from "./shortcuts.ts";

/** 快捷键要做的那几件事（会话层注入真动作，测试注入替身）。 */
export interface GlobalKeyDeps {
  /** 有弹窗开着吗（开着时一律不认界面快捷键） */
  dialogOpen: () => boolean;
  zenOn: () => boolean;
  fullscreenOn: () => boolean;
  toggleZen: () => void;
  toggleFullscreen: () => void;
  /** 退出专注：专注与全屏一起收（Esc 的语义是"回到常规"） */
  exitFocus: () => void;
  prevChapter: () => void;
  nextChapter: () => void;
  /** 立刻落盘：不等防抖（内容没变时是空转，可以放心按） */
  saveNow: () => void;
  /** 在当前章后面新建一章并开写（与目录树的「+」走同一条路） */
  newChapter: () => void;
  openSettings: () => void;
  openShelf: () => void;
}

/** 命中之后干什么（抽出来是为了能脱离事件对象单测）。 */
export function runShortcut(id: ShortcutId, deps: GlobalKeyDeps): void {
  switch (id) {
    case "zen":
      deps.toggleZen();
      return;
    case "fullscreen":
      deps.toggleFullscreen();
      return;
    case "exit-focus":
      deps.exitFocus();
      return;
    case "prev-chapter":
      deps.prevChapter();
      return;
    case "next-chapter":
      deps.nextChapter();
      return;
    case "save-now":
      deps.saveNow();
      return;
    case "new-chapter":
      deps.newChapter();
      return;
    case "settings":
      deps.openSettings();
      return;
    case "shelf":
      deps.openShelf();
      return;
  }
}

/** 把动作表接到目标上；返回**解绑函数**（组件卸载时必须调用）。 */
export function attachGlobalKeys(deps: GlobalKeyDeps, target: KeyTarget): () => void {
  return attachShortcuts(
    target,
    (event) =>
      matchShortcut(event, {
        dialogOpen: deps.dialogOpen(),
        inTextField: isTextFieldTarget(event.target),
        zenOn: deps.zenOn(),
        fullscreenOn: deps.fullscreenOn(),
      }),
    (id) => runShortcut(id, deps),
  );
}
