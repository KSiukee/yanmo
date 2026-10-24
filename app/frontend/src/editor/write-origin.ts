// 「这一笔是怎么写进来的」——**壳的观测**，核心只记它说的。
//
// 只有壳看得见三件事：敲了键、粘贴 / 拖放了一段、还是软件自己改了字（排版清理、
// 作答落章）。核心拿不到这些（击键不跨进程），所以：
//
// 1. **没看见就不报**：`take()` 返回 `null`，核心如实记「未记录」——绝不当成打字；
// 2. **一笔里混了几种就记「混合」**：不猜各占多少（落盘窗口 250ms，通常只有一种）；
// 3. **落盘失败要把观测放回去**：字还在手上，来源就不能先丢了（见 `save-transport.ts`）。
//
// 纯逻辑、不碰 DOM：`watchWriteSource` 才碰 DOM，两件事分开测。

import type { SaveOrigin } from "../api/source.ts";

/** 壳能直接观测到的三种来源（`mixed` 是"几种一起"的合成，不从外面报）。 */
export type WriteKind = "typed" | "pasted" | "system";

/** 一次落盘窗口里看见的来源。 */
export class WriteOrigins {
  /** 看见过的码；`mixed` 只会在 `restore` 时作为"当时已经混了"的浓缩出现。 */
  private kinds = new Set<string>();
  private clipboardChars = 0;

  /** 敲了键（含输入法上屏）。 */
  note(kind: WriteKind): void {
    this.kinds.add(kind);
  }

  /** 粘贴 / 拖放：顺带记下壳观测到的 plain text 字数（只作证据）。 */
  notePaste(chars: number): void {
    this.kinds.add("pasted");
    this.clipboardChars += Math.max(0, Math.trunc(chars) || 0);
  }

  /** 撤回一条记错的观测（例如插入被拒绝、其实什么都没发生）。 */
  forget(kind: WriteKind): void {
    this.kinds.delete(kind);
  }

  /**
   * 取走这一笔的观测。**空集合返回 `null`**（＝什么都没看见，由核心记未记录）。
   *
   * 取走之后计数清零：同一批观测只属于它发生的那一笔。
   */
  take(): SaveOrigin | null {
    if (this.kinds.size === 0) {
      this.clipboardChars = 0;
      return null;
    }
    const kind = this.kinds.size === 1 ? [...this.kinds][0] : "mixed";
    const clipboardChars = this.clipboardChars;
    this.kinds.clear();
    this.clipboardChars = 0;
    return clipboardChars > 0 ? { kind, clipboard_chars: clipboardChars } : { kind };
  }

  /**
   * 这一笔没落成：把观测放回去（下一笔连同它的字一起记）。
   *
   * `mixed` 无法还原成原来那几种——"当时已经混了"就是当时能说的全部。
   */
  restore(origin: SaveOrigin | null): void {
    if (!origin) return;
    this.kinds.add(origin.kind);
    this.clipboardChars += Math.max(0, origin.clipboard_chars ?? 0);
  }
}

/** 这一笔要不要跟正文一起送出去（测试与调试用）。 */
export function isEmpty(origin: SaveOrigin | null): boolean {
  return origin === null;
}

/** 键盘事件 → 算不算"落了一个字"。
 *
 * - 带 Ctrl / Meta / Alt 的一律不算（撤销、粘贴、保存都是命令，不是打字）；
 * - `key.length === 1`：字母 / 数字 / 标点 / 空格——真会落进正文的那些；
 * - 回车只加段落分隔（不落字），方向键 / 删除键也不落字；
 * - 输入法组字走 `compositionstart`，不靠 keydown（组字时 key 是 `Process`）。
 */
export function isTypingKey(event: {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  isComposing?: boolean;
}): boolean {
  if (event.ctrlKey || event.metaKey || event.altKey) return false;
  if (event.isComposing) return false;
  return event.key.length === 1;
}
