// 备份的状态与触发：设置页看什么、什么时候自动做、那条小条要不要弹。
//
// 三条分寸（与别处一致）：
// - **业务在核心**（快照、体检、保留、账本），这里只管"什么时候叫它"和"显示成什么"；
// - **失败绝不阻断写作与关窗**：自动备份失败只留一句话，弹都不弹；
// - 传输是注入的：可以脱离界面与核心单测。

import { ref, type Ref } from "vue";

import type { BackupConfig, BackupReport, BackupStatus } from "../api/core";

/** 备份要用的几个动作（会话层注入真命令，测试注入替身）。 */
export interface BackupTransport {
  status: (tz_offset_minutes: number) => Promise<BackupStatus>;
  write: (config: BackupConfig) => Promise<BackupConfig>;
  run: (tz_offset_minutes: number) => Promise<BackupReport>;
}

export interface BackupOptions {
  transport: BackupTransport;
  /** 本地时区相对 UTC 的分钟偏移（东八区 = +480）——核心不猜时区 */
  tzOffsetMinutes?: () => number;
  onError?: (message: string) => void;
}

export interface BackupState {
  visible: Ref<boolean>;
  /** 现状（没读回来之前是 null） */
  status: Ref<BackupStatus | null>;
  /** 最近一次备份的结果（逐目标） */
  lastReport: Ref<BackupReport | null>;
  busy: Ref<boolean>;
  error: Ref<string | null>;
  /** 打开设置页（打开前重读一次，免得显示旧值） */
  open: () => Promise<void>;
  close: () => void;
  /** 读一次现状（启动时用） */
  load: () => Promise<void>;
  /** 存配置（写完回读） */
  save: (config: BackupConfig) => Promise<void>;
  /** 立即备份 */
  runNow: () => Promise<void>;
  /** 「不用了」：拒过就不再自动弹（设置页仍常驻一行建议） */
  dismissTip: () => Promise<void>;
  /**
   * 每日首次启动：今天还没成功备份过、又有目标 → 自动做一次。
   *
   * **失败静默**（只记在 lastReport 里）：自动备份不该在作者一开软件时就报错打扰。
   */
  onStart: () => Promise<void>;
  /** 正常关窗时异步做一次（不拖慢退出；来不及做完就算了，下次启动还会做）。 */
  onClose: () => void;
}

/** 东八区这类偏移：JS 给的是"落后 UTC 多少分钟"，核心要的是"超前多少"。 */
export function localOffsetMinutes(now: Date = new Date()): number {
  return -now.getTimezoneOffset();
}

export function useBackup(options: BackupOptions): BackupState {
  const visible = ref(false);
  const status = ref<BackupStatus | null>(null);
  const lastReport = ref<BackupReport | null>(null);
  const busy = ref(false);
  const error = ref<string | null>(null);
  const tz = () => options.tzOffsetMinutes?.() ?? localOffsetMinutes();
  const report = (e: unknown) => {
    const message = e instanceof Error ? e.message : String(e);
    options.onError?.(message);
    return message;
  };

  async function load(): Promise<void> {
    try {
      status.value = await options.transport.status(tz());
      error.value = null;
    } catch (e) {
      status.value = null; // 读不到就先不显示（别拿旧值当真）
      error.value = report(e);
    }
  }

  async function save(config: BackupConfig): Promise<void> {
    if (busy.value) return;
    busy.value = true;
    try {
      const saved = await options.transport.write(config);
      // 写完回读一次现状：账本/盘可能同时变了，一次拿全
      status.value = await options.transport.status(tz());
      error.value = saved ? null : error.value;
    } catch (e) {
      error.value = report(e);
    } finally {
      busy.value = false;
    }
  }

  async function runNow(): Promise<void> {
    if (busy.value) return;
    busy.value = true;
    try {
      lastReport.value = await options.transport.run(tz());
      status.value = await options.transport.status(tz());
      error.value = null;
    } catch (e) {
      error.value = report(e);
    } finally {
      busy.value = false;
    }
  }

  return {
    visible,
    status,
    lastReport,
    busy,
    error,
    load,
    save,
    runNow,
    open: async () => {
      if (status.value === null) await load();
      visible.value = true;
    },
    close: () => {
      visible.value = false;
    },
    dismissTip: async () => {
      const current = status.value;
      if (!current) return;
      await save({ ...current.config, tip_dismissed: true });
    },
    onStart: async () => {
      await load();
      const current = status.value;
      if (!current) return; // 读不到现状就不自动做（宁可什么都不做，也别猜）
      if (!current.config.auto_on_start) return;
      if (current.config.targets.length === 0) return;
      // 今天还没有任何一处成功过 → 做一次（已经做过就不重复打扰）
      const doneToday = current.targets.some((t) => t.last_success === current.today);
      if (doneToday) return;
      try {
        lastReport.value = await options.transport.run(tz());
        await load();
      } catch {
        // 自动备份失败不打扰作者：账本里记着，设置页看得到
      }
    },
    onClose: () => {
      const current = status.value;
      if (!current || !current.config.auto_on_close) return;
      if (current.config.targets.length === 0) return;
      // 异步：关窗不等它（来不及做完就算了，下次启动还会做）
      void options.transport.run(tz()).catch(() => {});
    },
  };
}
