// 全局快捷键：**键位只在表里定义一次**，匹配与拦截都在这里。
//
// 为什么要有这一层：键位散在各个组件里的话，"这个键到底谁管"永远说不清，
// 更麻烦的是会跟输入法、与编辑器抢键——那种毛病在真机上表现为"写着写着突然换了章"。
//
// 四条硬性分寸（都是"别抢作者的键"）：
// 1. **输入法组字中一律不认**（`isComposing`）：组字时的字母与 Ctrl 是给输入法用的；
// 2. **焦点在输入框（`input` / `textarea`）里时只认带修饰键的组合**：作者在搜索框、
//    笔记框里打字，不该因为按了个 F9 就换布局；
// 3. **弹窗开着时一律不认**（含 Esc——Esc 归弹窗自己）：否则一下 Esc 会既关弹窗又退专注；
// 4. **可打印键一律不劫持**：表里只有功能键（F9 / F11 / Esc）与带修饰键的组合，
//    绝不出现"单个字母/数字"这种会吃掉正文输入的键。
//
// 另有一条来自真机经验的分寸：**按住不放不连发**（`repeat` 一律不认）——
// 开关类动作按一下切一次才对；切章是"落盘 + 换内容"的重动作，也不该被长按灌进来。

export type ShortcutId =
  | "zen"
  | "fullscreen"
  | "prev-chapter"
  | "next-chapter"
  | "save-now"
  | "new-chapter"
  | "settings"
  | "shelf"
  /** Esc：先关悬浮卡片（专注模式下的大纲等），没有卡片才退专注 */
  | "close-panel"
  /** Esc：退出专注（顺手把全屏也收了），"回到常规"一个键说完 */
  | "exit-focus";

/** 键位表里的一条。`ctrl` 在 mac 上也认 Command（metaKey），一套键位两边通用。 */
export interface ShortcutEntry {
  id: ShortcutId;
  /** 主键：单个字母写小写；功能键写 `F9` 这种规范名；方向键写 `ArrowLeft` 这种 DOM 名 */
  key: string;
  ctrl?: boolean;
  alt?: boolean;
  shift?: boolean;
}

/**
 * 键位表——**只有这一处**。
 *
 * 将来要做"键位自定义面板"，就是把这张表变成可读写的（面板读写同一张表，不会两处对不上）。
 */
export const SHORTCUTS: readonly ShortcutEntry[] = [
  { id: "fullscreen", key: "F11" },
  { id: "zen", key: "F9" },
  { id: "prev-chapter", key: "ArrowLeft", ctrl: true, alt: true },
  { id: "next-chapter", key: "ArrowRight", ctrl: true, alt: true },
  { id: "save-now", key: "s", ctrl: true },
  { id: "new-chapter", key: "n", ctrl: true },
  { id: "settings", key: ",", ctrl: true },
  { id: "shelf", key: "b", ctrl: true },
];

/** 键盘事件里我们要看的那几项（只取这几项，测试给个普通对象就能跑）。 */
export interface KeyLike {
  key: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
  /** 输入法组字中（真机上这是最要命的一项） */
  isComposing?: boolean;
  /** 系统判定为"长按连发" */
  repeat?: boolean;
}

/** 判定时要知道的界面状态（由会话层现取，避免这里反向依赖会话）。 */
export interface MatchContext {
  /** 有弹窗开着吗 */
  dialogOpen: boolean;
  /** 焦点在输入框里吗（看 `isTextFieldTarget`） */
  inTextField: boolean;
  /** 专注模式开着吗（Esc 只在开着时才管） */
  zenOn: boolean;
  /** 全屏开着吗（Esc 也管它） */
  fullscreenOn: boolean;
  /** 专注模式下的悬浮卡片开着吗（Esc 的第一优先：先关卡，再退专注） */
  panelOpen: boolean;
}

/** 焦点是不是落在**输入框**里。 */
export function isTextFieldTarget(target: unknown): boolean {
  const element = target as { tagName?: unknown } | null | undefined;
  const tag = element?.tagName;
  if (typeof tag !== "string") return false;
  const upper = tag.toUpperCase();
  // ⚠️ 只认 input / textarea：**正文本体是 contenteditable，不算输入框**——
  // 算进去的话，作者在正文里按 Esc 就退不出专注了（而 Esc 最主要的用法正是在正文里）。
  return upper === "INPUT" || upper === "TEXTAREA";
}

/** 按下去的是哪件事；没命中返回 null（**不 preventDefault，让键照常走**）。 */
export function matchShortcut(event: KeyLike, context: MatchContext): ShortcutId | null {
  if (event.isComposing) return null;
  if (event.repeat) return null;
  if (context.dialogOpen) return null;

  const key = normalizeKey(event.key);
  const ctrl = event.ctrlKey || event.metaKey;

  if (key === "Escape") {
    if (context.inTextField) return null; // 输入框里的 Esc 是"取消输入"
    // 一层一层退：先收悬浮卡片，再退专注/全屏。反过来的话，一下 Esc 会连退两层，
    // 作者的预期是"关掉眼前这张"——退专注得再按一次。
    if (context.panelOpen) return "close-panel";
    return context.zenOn || context.fullscreenOn ? "exit-focus" : null;
  }

  // 输入框里只认带修饰键的组合（免得打字时被功能键打断）
  if (context.inTextField && !ctrl && !event.altKey) return null;

  const hit = SHORTCUTS.find(
    (entry) =>
      normalizeKey(entry.key) === key &&
      Boolean(entry.ctrl) === ctrl &&
      Boolean(entry.alt) === event.altKey &&
      Boolean(entry.shift) === event.shiftKey,
  );
  return hit ? hit.id : null;
}

/** 键位给人看的样子（按钮 title 用；键位改表，提示自动跟着走）。 */
export function shortcutKeys(id: ShortcutId): string {
  // Esc 不进表：它要多看一条"有没有东西可退"（见 matchShortcut），但提示也得有个出处
  if (id === "exit-focus") return ESCAPE_HINT;
  const entry = SHORTCUTS.find((item) => item.id === id);
  if (!entry) return "";
  const parts: string[] = [];
  if (entry.ctrl) parts.push("Ctrl");
  if (entry.alt) parts.push("Alt");
  if (entry.shift) parts.push("Shift");
  parts.push(displayKey(entry.key));
  return parts.join("+");
}

/** Esc 的显示名（`exit-focus` 不在表里，原因见 `matchShortcut`）。 */
export const ESCAPE_HINT = "Esc";

/** 能被 `attachShortcuts` 接上的目标（`window` 天然满足；测试给个假的就能跑）。 */
export interface KeyTarget {
  addEventListener: (type: string, listener: (event: Event) => void, capture?: boolean) => void;
  removeEventListener: (type: string, listener: (event: Event) => void, capture?: boolean) => void;
}

/**
 * 把"匹配 → 动作"接到目标上；返回**解绑函数**（卸载时必须调用，否则监听会越挂越多）。
 *
 * `resolve` 由调用方给（它要现取界面状态，见 `MatchContext`），这里只负责：
 * 命中就**在捕获阶段拦下**——WebView2 自己也认 Ctrl+S / Ctrl+N / F11，
 * 不拦的话会同时触发两套（存网页对话框、开新窗口、浏览器全屏）。
 */
export function attachShortcuts(
  target: KeyTarget,
  resolve: (event: KeyboardEvent) => ShortcutId | null,
  run: (id: ShortcutId) => void,
): () => void {
  const onKeyDown = (event: Event) => {
    const keyboard = event as KeyboardEvent;
    const id = resolve(keyboard);
    if (id === null) return;
    keyboard.preventDefault();
    keyboard.stopPropagation();
    run(id);
  };
  target.addEventListener("keydown", onKeyDown, true);
  return () => target.removeEventListener("keydown", onKeyDown, true);
}

function normalizeKey(key: string): string {
  // 单字符键统一小写：真机上 CapsLock / Shift 会让 `key` 变成大写，
  // 不归一化就会出现"按了没反应"这种最难查的毛病（Shift 组合另有一套判定）
  return key.length === 1 ? key.toLowerCase() : key;
}

function displayKey(key: string): string {
  if (key === "ArrowLeft") return "←";
  if (key === "ArrowRight") return "→";
  if (key === "ArrowUp") return "↑";
  if (key === "ArrowDown") return "↓";
  return key.length === 1 ? key.toUpperCase() : key;
}
