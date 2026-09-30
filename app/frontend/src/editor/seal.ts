// 加密备份包的界面状态：开关、口令（只在内存）、恢复码（只在开启那一次给）。
//
// 三条分寸（与别处一致）：
// - **业务在核心与壳**（派生、加密、生成恢复码都在那边），这里只管"什么时候叫它"与"显示成什么"；
// - **口令不在这里校验**：`unlock` 只是把它交上去记住，对不对要等真去开一个包才知道——
//   界面要如实说这一句（"看起来记住了"不等于"口令是对的"）；
// - **恢复码必须被作者确认过才收起**：那是唯一一次能抄下来的机会（包里取不回来），
//   界面不许替作者点掉那张卡片。
//
// 传输是注入的：可以脱离界面与核心单测（见 seal.test.ts）。

import { ref, type Ref } from "vue";

import type { SealStatus } from "../api/seal";

/** 加密那一格要用的几个动作（会话层注入真命令，测试注入替身）。 */
export interface SealTransport {
  status: () => Promise<SealStatus>;
  /** 开启：**只有本机还没有过恢复码时**才返回那一串（已经有就是 null） */
  enable: (passphrase: string) => Promise<string | null>;
  disable: () => Promise<SealStatus>;
  /** 给口令（放内存）：**这里验不了对错**，返回的是"现在的现状" */
  unlock: (passphrase: string) => Promise<SealStatus>;
  lock: () => Promise<void>;
}

export interface SealOptions {
  transport: SealTransport;
  onError?: (message: string) => void;
}

export interface SealState {
  /** 现状（没读回来之前是 null） */
  status: Ref<SealStatus | null>;
  busy: Ref<boolean>;
  error: Ref<string | null>;
  /** 恢复码那张**必须确认**的卡片开着没有 */
  showRecovery: Ref<boolean>;
  /** 这一次拿到的恢复码（空串＝这一次没有新码） */
  recoveryCode: Ref<string>;
  /** 开启成功但本机已经有一把恢复码（没给新码）——界面上说一句"接着用原来那把" */
  recoveryExists: Ref<boolean>;
  /** 读一次现状 */
  load: () => Promise<void>;
  /** 开启加密（两次一致、非空这些由界面先拦，见 SealSettings.vue） */
  enable: (passphrase: string) => Promise<void>;
  disable: () => Promise<void>;
  /** 给口令（放内存）：**不校验对错** */
  unlock: (passphrase: string) => Promise<void>;
  /** 把口令从内存里撤掉 */
  lock: () => Promise<void>;
  /** 作者说"我已抄到别处"——**这才**收起那张卡片 */
  confirmRecovery: () => void;
}

export function useSeal(options: SealOptions): SealState {
  const status = ref<SealStatus | null>(null);
  const busy = ref(false);
  const error = ref<string | null>(null);
  const showRecovery = ref(false);
  const recoveryCode = ref("");
  const recoveryExists = ref(false);
  const report = (e: unknown) => {
    const message = e instanceof Error ? e.message : String(e);
    options.onError?.(message);
    return message;
  };

  async function load(): Promise<void> {
    try {
      status.value = await options.transport.status();
      error.value = null;
    } catch (e) {
      status.value = null; // 读不到就先不显示（别拿旧值当真）
      error.value = report(e);
    }
  }

  /**
   * 四个动作同一个骨架：**动作 → 读回现状 → 松开 busy**。
   *
   * 为什么要读回来：界面显示的是壳报的事实（内存里有没有口令、开关开着没有），
   * 自己推断"应该已经开了"迟早会跟真实状态走散。动作返回 null（`lock` 不返回现状）时读一次。
   */
  async function withBusy(action: () => Promise<SealStatus | null>): Promise<void> {
    if (busy.value) return;
    busy.value = true;
    try {
      const next = await action();
      if (next) status.value = next;
      else await load();
      error.value = null;
    } catch (e) {
      error.value = report(e);
    } finally {
      busy.value = false;
    }
  }

  return {
    status,
    busy,
    error,
    showRecovery,
    recoveryCode,
    recoveryExists,
    load,
    enable: async (passphrase: string) => {
      recoveryExists.value = false;
      await withBusy(async () => {
        const code = await options.transport.enable(passphrase);
        if (code === null) {
          // 本机已经有一把：不重复给新码（换掉它会让以前那些包失去第二把钥匙）
          recoveryExists.value = true;
        } else {
          recoveryCode.value = code;
          showRecovery.value = true; // **必须确认过**才收起（见 confirmRecovery）
        }
        return options.transport.status();
      });
    },
    disable: async () => {
      await withBusy(async () => {
        showRecovery.value = false;
        recoveryCode.value = "";
        recoveryExists.value = false;
        return options.transport.disable();
      });
    },
    unlock: async (passphrase: string) => {
      await withBusy(() => options.transport.unlock(passphrase));
    },
    lock: async () => {
      await withBusy(async () => {
        await options.transport.lock();
        return null; // 这条不返回现状：自己在 withBusy 里读一次
      });
    },
    confirmRecovery: () => {
      showRecovery.value = false;
      recoveryCode.value = "";
    },
  };
}
