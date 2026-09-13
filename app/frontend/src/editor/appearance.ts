// 外观 / 写作行为偏好：**全局一份、跟人不跟书**（书可以单独覆盖，结构留位）。
//
// 三条分寸：
// - **只存改过的项**：库里是"空档"，读的时候才落默认值（默认值只写在核心那一处），
//   所以这里读到的永远是"已经落好值"的一份；
// - **没读回来之前不拿默认值当真相**：`values` 先是 null，读失败就一直 null——
//   界面此时按"不抢焦点"处理（安全那一边），而不是猜一个默认值往下走；
// - 这些偏好**只影响显示与手感**，不进导出、不改动正文一个字。
//
// 只认接口不认具体命令（真命令在会话层注入）：可以脱离界面与核心单测。

import { ref, type Ref } from "vue";

import type { Appearance, AppearancePatch } from "../api/core";

/** 偏好要用的几个动作（会话层注入真命令，测试注入替身）。 */
export interface AppearanceTransport {
  read: (work_id: number | null) => Promise<Appearance>;
  write: (work_id: number | null, patch: AppearancePatch) => Promise<Appearance>;
  reset: (work_id: number | null) => Promise<Appearance>;
}

export interface AppearanceOptions {
  transport: AppearanceTransport;
  /** 当前作品（null = 还没打开书）——"只设这本书"要用它 */
  workId: Ref<number | null>;
  onError?: (message: string) => void;
}

/** 设命名规则时写哪一层：全局默认 / 只设当前这本书。 */
export type NamingTarget = "default" | "work";

export interface AppearanceState {
  /** 库里那份（**读失败或还没读是 null**）；设置面板据此显示 */
  values: Ref<Appearance | null>;
  /** **当前作品生效的那一份**（全局 + 每书覆盖合并后）；没打开书时是 null */
  workValues: Ref<Appearance | null>;
  /** 设置面板开着没有 */
  visible: Ref<boolean>;
  busy: Ref<boolean>;
  /** 读一次（启动时用；失败不打扰写作，按安全那一边走） */
  load: () => Promise<void>;
  /** 打开设置面板（打开前重读一次，免得显示旧值） */
  open: () => Promise<void>;
  close: () => void;
  /** 改一项：写完回读，界面显示的永远是库里那份 */
  setJumpToEnd: (value: boolean) => Promise<void>;
  /** 引号用哪一套（排版清理里选的，选一次就记住） */
  setQuoteStyle: (value: string) => Promise<void>;
  /** 回到默认（核心的默认值） */
  resetToDefault: () => Promise<void>;
  /** 跟当前作品重读一次（打开章节、换书时用） */
  loadWork: () => Promise<void>;
  /** 设命名规则：`target` 决定写全局默认还是只写这本书；值传 `"auto"` = 清掉那一层 */
  setNaming: (value: string, target: NamingTarget) => Promise<void>;
}

export function useAppearance(options: AppearanceOptions): AppearanceState {
  const values = ref<Appearance | null>(null);
  const workValues = ref<Appearance | null>(null);
  const visible = ref(false);
  const busy = ref(false);
  const report = (error: unknown) => {
    options.onError?.(error instanceof Error ? error.message : String(error));
  };

  /** 一次动作统一收口：忙标记 + 写回读 + 报错。 */
  async function act(op: () => Promise<Appearance>): Promise<void> {
    if (busy.value) return;
    busy.value = true;
    try {
      values.value = await op();
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  const state: AppearanceState = {
    values,
    workValues,
    visible,
    busy,
    load: async () => {
      try {
        values.value = await options.transport.read(null);
      } catch {
        values.value = null; // 读不到就一直 null：界面按"不抢焦点"处理
      }
    },
    open: async () => {
      if (values.value === null) await act(() => options.transport.read(null));
      // 打开设置时把"这本书生效的那一份"也重读一次（每书覆盖的项要显示当前值）
      await state.loadWork();
      visible.value = true;
    },
    close: () => {
      visible.value = false;
    },
    setJumpToEnd: (value) =>
      act(() => options.transport.write(null, { jump_to_end_on_latest: value })),
    setQuoteStyle: (value) => act(() => options.transport.write(null, { quote_style: value })),
    loadWork: async () => {
      const work_id = options.workId.value;
      if (work_id === null) {
        workValues.value = null;
        return;
      }
      try {
        workValues.value = await options.transport.read(work_id);
      } catch {
        workValues.value = null; // 读不到就不显示"这本书生效的是哪一档"，不猜
      }
    },
    setNaming: async (value, target) => {
      const work_id = target === "work" ? options.workId.value : null;
      if (target === "work" && work_id === null) return;
      await act(async () => {
        const written = await options.transport.write(work_id, { naming: value });
        // 这一本生效的那一份也跟着刷（设置面板同时显示"现在生效"）
        workValues.value = work_id === null ? workValues.value : written;
        return written;
      });
    },
    resetToDefault: () => act(() => options.transport.reset(null)),
  };
  return state;
}
