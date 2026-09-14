// 版本快照：列一列、比一比、留一版、删一版、回滚。
//
// 三条分寸与别处一致：
// - **滚动保留、手动版本不清、回滚先留底**的语义全在核心，这里只转发；
// - **回滚会覆盖正文**：动手前先把手上这份落盘（`beforeRestore`）——存不下去就不该覆盖；
// - 回滚换掉的是正文：交回 `onRestored`，由会话层换编辑器内容并重建落盘基准。
//
// 只认接口不认具体命令（真命令在会话层注入）：这套状态机可以脱离界面与核心单测。

import { ref, type Ref } from "vue";

import type { SnapshotDiff, SnapshotRestoreAck, SnapshotSummary } from "../api/core";
import { has, t } from "../locales/index.ts";

/** 这条版本是怎么来的（核心只给 reason 码，句子在字典里）。
 *
 * **认不出来的码就原样露出来**——与字典那条"查不到就返回键"同一条纪律：
 * 宁可屏幕上出现一个 `snapshots.reason_没见过的码`（一眼看得见、改起来快），
 * 也不要拿一句像模像样的话顶上去。以前这里兜底成「自动留的」，于是**真正没见过的来源**
 * 会显示成一个正常来源的名字（假安心）；而且 `auto` 这个值根本没有产出点——
 * 那条兜底文案本身就是个死值（工具仓的 `qa/audit_states.py` 会把这种对不上抓出来）。
 */
export function snapshotReason(reason: string): string {
  const key = `snapshots.reason_${reason}`;
  return has(key) ? t(key) : reason;
}

/** 版本历史要用的几个动作（会话层注入真命令，测试注入替身）。 */
export interface SnapshotTransport {
  list: (node_id: number) => Promise<SnapshotSummary[]>;
  diff: (node_id: number, snapshot_id: number) => Promise<SnapshotDiff>;
  /** 手动留一版；内容没变时核心返回 null */
  keep: (node_id: number) => Promise<number | null>;
  drop: (snapshot_id: number) => Promise<void>;
  restore: (snapshot_id: number) => Promise<SnapshotRestoreAck>;
}

export interface SnapshotsOptions {
  transport: SnapshotTransport;
  /** 当前正在写的那一章；没有就什么都做不了 */
  nodeId: Ref<number | null>;
  /** 回滚前把手上这份落盘（失败会抛异常——那就别覆盖） */
  beforeRestore: () => Promise<void>;
  /** 正文已换成回滚后的那一版：会话层据此换编辑器内容 + 重建落盘基准 */
  onRestored: (ack: SnapshotRestoreAck) => void;
  onError?: (message: string) => void;
}

export interface Snapshots {
  entries: Ref<SnapshotSummary[]>;
  visible: Ref<boolean>;
  busy: Ref<boolean>;
  /** 上一次动作的交代（"已留一版""已回滚"） */
  note: Ref<string>;
  /** 正在比的是哪一条（null = 没选） */
  selected: Ref<number | null>;
  /** 选中的那一条与当前正文的差异 */
  diff: Ref<SnapshotDiff | null>;
  /** 打开 / 收起（打开时刷新一次，并默认比最新那条） */
  toggle: () => void;
  close: () => void;
  refresh: () => Promise<void>;
  /** 选中一条并拉出它与当前稿的差异 */
  compare: (entry: SnapshotSummary) => Promise<void>;
  keep: () => Promise<void>;
  drop: (entry: SnapshotSummary) => Promise<void>;
  restore: (entry: SnapshotSummary) => Promise<void>;
}

export function useSnapshots(options: SnapshotsOptions): Snapshots {
  const entries = ref<SnapshotSummary[]>([]);
  const visible = ref(false);
  const busy = ref(false);
  const note = ref("");
  const selected = ref<number | null>(null);
  const diff = ref<SnapshotDiff | null>(null);

  const report = (error: unknown) =>
    options.onError?.(error instanceof Error ? error.message : String(error));

  async function loadDiff(node_id: number, snapshot_id: number): Promise<void> {
    diff.value = await options.transport.diff(node_id, snapshot_id);
  }

  async function refresh(): Promise<void> {
    const node_id = options.nodeId.value;
    if (node_id === null) {
      entries.value = [];
      selected.value = null;
      diff.value = null;
      return;
    }
    try {
      entries.value = await options.transport.list(node_id);
    } catch (error) {
      report(error);
      return;
    }
    const chosen = selected.value;
    if (chosen === null || !entries.value.some((entry) => entry.id === chosen)) {
      // 选中的那条不在了（被滚动清掉 / 手动删了）：差异视图跟着收起
      selected.value = null;
      diff.value = null;
      return;
    }
    // 选中的还在：**当前正文可能已经变了**（留一版、回滚之后都会走到这里），差异要重拉
    try {
      await loadDiff(node_id, chosen);
    } catch (error) {
      report(error);
      diff.value = null;
    }
  }

  /** 一次动作统一收口：忙标记 + 刷新列表 + 报错（与回收站同一套写法）。 */
  async function act(op: () => Promise<string>): Promise<void> {
    if (busy.value) return;
    busy.value = true;
    try {
      note.value = await op();
      await refresh();
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  async function compare(entry: SnapshotSummary): Promise<void> {
    const node_id = options.nodeId.value;
    if (node_id === null) return;
    selected.value = entry.id;
    try {
      await loadDiff(node_id, entry.id);
    } catch (error) {
      report(error);
      diff.value = null;
    }
  }

  return {
    entries,
    visible,
    busy,
    note,
    selected,
    diff,
    toggle: () => {
      visible.value = !visible.value;
      note.value = "";
      if (!visible.value) return;
      void refresh().then(() => {
        // 一进来就比最新那一版：多数时候作者想看的就是"刚丢的那点东西去哪了"
        const latest = entries.value[0];
        if (latest) void compare(latest);
      });
    },
    close: () => {
      visible.value = false;
    },
    refresh,
    compare,
    keep: () =>
      act(async () => {
        const node_id = options.nodeId.value;
        if (node_id === null) return "";
        const id = await options.transport.keep(node_id);
        if (id === null) return t("snapshots.kept_same");
        selected.value = id;
        return t("snapshots.kept");
      }),
    drop: (entry) =>
      act(async () => {
        await options.transport.drop(entry.id);
        if (selected.value === entry.id) {
          selected.value = null;
          diff.value = null;
        }
        return t("snapshots.dropped");
      }),
    restore: (entry) =>
      act(async () => {
        const node_id = options.nodeId.value;
        if (node_id === null) return "";
        // ① 先落盘：存不下去就不覆盖（核心还会替它留一份底）
        await options.beforeRestore();
        // ② 回滚，并把换回来的正文交给会话层
        const ack = await options.transport.restore(entry.id);
        options.onRestored(ack);
        selected.value = null;
        diff.value = null;
        return t("snapshots.restored");
      }),
  };
}
