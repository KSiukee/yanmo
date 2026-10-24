// 落盘传输：把**壳的观测**（这一笔是键入 / 粘贴 / 混合）跟正文一起交给核心。
//
// 为什么要单独一层：Autosave 只管"什么时候写、写哪一版"，不该知道来源；
// 而来源有一件事与落盘成败绑在一起——**写失败时观测必须放回去**。
// 否则：粘贴 → 落盘失败 → 重试时来源已经丢了 → 核心把那一千字记成「未记录」。
// 这个类只有这一条纪律，所以能单独测（三个命令由调用方注入，不在这一层 import Tauri）。

import type { SaveAck } from "../api/core.ts";
import type { SaveOrigin, SourceBreakdown } from "../api/source.ts";
import type { AutosaveTransport } from "./autosave.ts";
import type { WriteOrigins } from "./write-origin.ts";

/** 落盘命令的形状（与 `api/core.ts` 里那个 `saveBody` 一致）。 */
export type SaveFn = (
  node_id: number,
  body: string,
  tz: number,
  origin?: SaveOrigin,
) => Promise<SaveAck>;
/** 读回校验用的指纹命令。 */
export type FingerprintFn = (node_id: number) => Promise<string>;
/** 抢救命令（落盘卡住 / 不一致时走它）。 */
export type EmergencyFn = (
  node_id: number,
  body: string,
  reason: string,
  tz: number,
  origin?: SaveOrigin,
) => Promise<SaveAck>;

export interface SaveTransportDeps {
  /** 这一章一个（换章换新的）：来源属于"这一笔"，不属于会话 */
  origins: WriteOrigins;
  /** 本地时区偏移（调用时现取：跨零点写的字也要算对日子） */
  tz: () => number;
  /** 每次落盘成功都把这一章最新的来源分布交回去（状态栏那一行） */
  onSources: (sources: SourceBreakdown) => void;
  save: SaveFn;
  fingerprint: FingerprintFn;
  emergency: EmergencyFn;
}

export function saveTransport(deps: SaveTransportDeps): AutosaveTransport {
  /** 取走观测 → 落盘 → 失败就放回去。**成功才清**，这是唯一一条纪律。 */
  async function withOrigin<T extends { sources: SourceBreakdown }>(
    run: (origin: SaveOrigin | undefined) => Promise<T>,
  ): Promise<T> {
    const origin = deps.origins.take();
    try {
      const ack = await run(origin ?? undefined);
      deps.onSources(ack.sources);
      return ack;
    } catch (error) {
      deps.origins.restore(origin);
      throw error;
    }
  }

  return {
    save: (node_id, body) => withOrigin((origin) => deps.save(node_id, body, deps.tz(), origin)),
    fingerprint: deps.fingerprint,
    emergency: (node_id, body, reason) =>
      withOrigin((origin) => deps.emergency(node_id, body, reason, deps.tz(), origin)),
  };
}
