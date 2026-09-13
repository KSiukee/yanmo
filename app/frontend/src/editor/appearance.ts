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
  onError?: (message: string) => void;
}

export interface AppearanceState {
  /** 库里那份（**读失败或还没读是 null**）；设置面板据此显示 */
  values: Ref<Appearance | null>;
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
}

export function useAppearance(options: AppearanceOptions): AppearanceState {
  const values = ref<Appearance | null>(null);
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

  return {
    values,
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
      visible.value = true;
    },
    close: () => {
      visible.value = false;
    },
    setJumpToEnd: (value) =>
      act(() => options.transport.write(null, { jump_to_end_on_latest: value })),
    setQuoteStyle: (value) => act(() => options.transport.write(null, { quote_style: value })),
    resetToDefault: () => act(() => options.transport.reset(null)),
  };
}
