// 分卷：**提议 → 在这里收卷 → 一键撤销**这一小段编排。
//
// 三条纪律：
// 1. **绝不打断写作**：提示只落在目录那一栏（一行安静的小字 + 两个按钮），不弹窗、不抢焦点——
//    要在他**自己要停下来**的时刻说（写完一章、换章的时候），写作中不插话；
// 2. **「再等等」不是白问**：第一次是"下一章再说"，**连着两次这一卷就不再提**；
// 3. 收卷 / 撤卷**只动结构**：正文一个字不动（核心那边有断言钉着）。
//
// 这里只认接口不认具体命令（真命令在会话层注入）：状态机可以脱离界面与核心单测。

import { ref, type Ref } from "vue";

import type { CloseVolumeReceipt, DissolveVolumeReceipt, VolumeOffer } from "../api/volumes";

/** 分卷要用到的三件事（会话层注入真命令，测试注入替身）。 */
export interface VolumeTransport {
  offer: (node_id: number) => Promise<VolumeOffer | null>;
  close: (node_id: number, title: string) => Promise<CloseVolumeReceipt>;
  dissolve: (volume_id: number) => Promise<DissolveVolumeReceipt>;
}

export interface VolumeDeps {
  transport: VolumeTransport;
  /** 收卷 / 撤卷之后：目录树重拉看得见的层 + 卷长口径重读（都在 directory 里收口） */
  afterChange: () => Promise<void>;
  /** 成卷时顺手起的第一章：把光标落过去接着写 */
  openChapter: (node_id: number) => Promise<void>;
  onError?: (message: string) => void;
}

/** 刚收好的那一卷（界面据此显示「一键撤销」那一行）。 */
export interface ClosedVolume {
  volume_id: number;
  /** 收卷点为止，这一卷有几章 */
  count: number;
}

export interface Volumes {
  /** 现在这一章后面要不要提一句收卷（null = 不提） */
  offer: Ref<VolumeOffer | null>;
  /** 刚收好的那一卷（撤销入口）；没刚收过就是 null */
  closed: Ref<ClosedVolume | null>;
  /** 切章之后问一句：这一章的收卷口径（只读一次库，便宜） */
  consider: (node_id: number | null) => Promise<void>;
  /** 换作品：提示与撤销记录都只对当前这本书，全部清掉 */
  reset: () => void;
  /** 「再等等」：这一次先不提 */
  snooze: () => void;
  /** 「在这里收卷」 */
  closeHere: () => Promise<void>;
  /** 「撤销」：把刚收的那一卷拆掉 */
  undo: () => Promise<void>;
}

/** 连着两次「再等等」，这一卷就不再提了（回答不能白问）。 */
const SNOOZE_LIMIT = 2;

export function useVolumes(deps: VolumeDeps): Volumes {
  const offer = ref<VolumeOffer | null>(null);
  const closed = ref<ClosedVolume | null>(null);
  /** 每一卷被打过几次「再等等」（根层那一段散章算 "root"） */
  const snoozed = new Map<string, number>();
  /** 当前提议的收卷点（哪一章）；没有提议时是 null */
  let boundary: number | null = null;
  /** 提问的流水号：切章很快，**慢回来的旧答复不许盖掉新的** */
  let asked = 0;
  let busy = false;

  const report = (error: unknown) => {
    deps.onError?.(error instanceof Error ? error.message : String(error));
  };
  const keyOf = (spot: VolumeOffer) => String(spot.container ?? "root");

  async function consider(node_id: number | null): Promise<void> {
    const ticket = ++asked;
    boundary = null;
    if (node_id === null) {
      offer.value = null;
      return;
    }
    try {
      const next = await deps.transport.offer(node_id);
      if (ticket !== asked) return; // 已经切到别的章了，这份答复过期
      const muted = next !== null && (snoozed.get(keyOf(next)) ?? 0) >= SNOOZE_LIMIT;
      offer.value = !muted && next !== null ? next : null;
      boundary = offer.value === null ? null : node_id;
    } catch (error) {
      report(error);
      offer.value = null;
    }
  }

  return {
    offer,
    closed,
    consider,
    // 换作品：把提示、撤销入口与"问过几次"的记忆一起清掉（别串到下一本书上）
    reset: () => {
      asked += 1; // 让飞在半路的答复作废
      offer.value = null;
      closed.value = null;
      boundary = null;
      snoozed.clear();
    },
    snooze: () => {
      const current = offer.value;
      if (current === null) return;
      const key = keyOf(current);
      snoozed.set(key, (snoozed.get(key) ?? 0) + 1);
      offer.value = null;
      boundary = null;
    },
    closeHere: async () => {
      const current = offer.value;
      const node = boundary;
      if (current === null || node === null || busy) return;
      busy = true;
      try {
        // 卷名留空 = 按位置渲染成「第 N 卷」；作者回头能在树上就地改名
        const receipt = await deps.transport.close(node, "");
        offer.value = null;
        boundary = null;
        closed.value = { volume_id: receipt.volume_id, count: current.count };
        await deps.afterChange();
        // 空卷写不了字：核心顺手起了第一章，这就把光标落过去接着写
        if (receipt.opened_chapter !== null) await deps.openChapter(receipt.opened_chapter);
      } catch (error) {
        report(error);
      } finally {
        busy = false;
      }
    },
    undo: async () => {
      const last = closed.value;
      if (last === null || busy) return;
      busy = true;
      try {
        await deps.transport.dissolve(last.volume_id);
        closed.value = null;
        await deps.afterChange();
      } catch (error) {
        report(error);
      } finally {
        busy = false;
      }
    },
  };
}
