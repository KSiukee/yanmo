// 从备份恢复的状态与动作：列出来源 → 看清会丢什么 → 确认换库。
//
// 三条分寸：
// - **业务在核心**（体检、留底、回滚、换库都在 `store::restore`），这里只管
//   "什么时候叫它"和"显示成什么"；
// - **换库前先落盘**：手上这一章先存下来——万一换库没成、原库回滚，
//   作者这一段就不至于留个缺口；
// - **体检不过就不给按**：按不按得动只看核心给的 `can_restore`，界面不自己判断
//   （"这份看着还行"是最危险的一种自信）。
//
// 传输是注入的：可以脱离界面与核心单测。

import { ref, type Ref } from "vue";

import type { RestoreOutcome, RestorePreview, RestoreSources } from "../api/core";
import type { UnsealedSource } from "../api/seal";
import { localOffsetMinutes } from "./backup.ts";

/** 加密备份包的文件后缀（与核心 `is_sealed_package` 同一口径：`研墨备份-….ympkg`）。 */
const SEALED_SUFFIX = ".ympkg";

/** 恢复要用的几个动作（会话层注入真命令，测试注入替身）。 */
export interface RestoreTransport {
  sources: () => Promise<RestoreSources>;
  preview: (source: string) => Promise<RestorePreview>;
  apply: (source: string, tz_offset_minutes: number) => Promise<RestoreOutcome>;
  /** 让作者挑一个库文件（返回 null = 取消了） */
  pick: (title: string, filter_label: string) => Promise<string | null>;
  /** 给加密包钥匙：解开 + 体检（返回解开的目录与结论）；钥匙不对就 reject */
  unlock: (source: string, kind: "passphrase" | "recovery", secret: string) => Promise<UnsealedSource>;
}

export interface RestoreOptions {
  transport: RestoreTransport;
  /** 本地时区相对 UTC 的分钟偏移（东八区 = +480）——核心不猜时区 */
  tzOffsetMinutes?: () => number;
  /** 换库之前先把手上这一章落盘；它失败就不该换库 */
  beforeApply: () => Promise<void>;
  onError?: (message: string) => void;
}

export interface RestoreState {
  visible: Ref<boolean>;
  /** 能恢复的备份包（没读回来之前是 null） */
  sources: Ref<RestoreSources | null>;
  /** 作者点中的那一份（包目录或库文件） */
  picked: Ref<string | null>;
  /** 选中那一份的体检结论 */
  preview: Ref<RestorePreview | null>;
  busy: Ref<boolean>;
  error: Ref<string | null>;
  /** 选中的是一份加密包：要钥匙（**这时候还没有体检结论**，见 `unlock`） */
  needKey: Ref<boolean>;
  /** 换库成功（窗口马上会重启，所以成功之后一直保持 busy） */
  done: Ref<RestoreOutcome | null>;
  open: () => Promise<void>;
  close: () => void;
  load: () => Promise<void>;
  /** 选中一份并体检（体检结论出来之前不显示"可以恢复"） */
  select: (source: string) => Promise<void>;
  pickDatabase: (title: string, filterLabel: string) => Promise<void>;
  /** 给加密包钥匙（口令 / 恢复码）：解开的目录与体检结论一起回来 */
  unlock: (kind: "passphrase" | "recovery", secret: string) => Promise<void>;
  apply: () => Promise<void>;
}

export function useRestore(options: RestoreOptions): RestoreState {
  const visible = ref(false);
  const sources = ref<RestoreSources | null>(null);
  const picked = ref<string | null>(null);
  const preview = ref<RestorePreview | null>(null);
  const needKey = ref(false);
  const busy = ref(false);
  const error = ref<string | null>(null);
  const done = ref<RestoreOutcome | null>(null);
  const tz = () => options.tzOffsetMinutes?.() ?? localOffsetMinutes();
  const report = (e: unknown) => {
    const message = e instanceof Error ? e.message : String(e);
    options.onError?.(message);
    return message;
  };

  async function load(): Promise<void> {
    try {
      sources.value = await options.transport.sources();
      error.value = null;
    } catch (e) {
      sources.value = null; // 读不到就先不列（别拿旧列表当真）
      error.value = report(e);
    }
  }

  /**
   * 这份来源是不是加密包：**清单在密文里**，拿它去体检壳会明确拒绝
   *（`backup.restore_sealed_needs_key`），所以要先给钥匙、不能发 preview。
   *
   * 两条判据都要：候选列表里壳标了 `sealed` 的，以及**从磁盘选进来的 `.ympkg`**
   *（那条路上没有清单可查，只能看扩展名——与壳 `is_sealed_package` 同一口径）。
   */
  function sealedSource(source: string): boolean {
    if (source.toLowerCase().endsWith(SEALED_SUFFIX)) return true;
    return sources.value?.packages.some((item) => item.path === source && item.sealed) ?? false;
  }

  async function select(source: string): Promise<void> {
    picked.value = source;
    preview.value = null; // 先清掉上一份的结论：宁可显示"还没体检"，也别把别人的结论挂在这份上
    needKey.value = sealedSource(source); // 加密包：先要钥匙，**不发 preview**
    if (needKey.value) {
      error.value = null;
      return;
    }
    busy.value = true;
    try {
      preview.value = await options.transport.preview(source);
      error.value = null;
    } catch (e) {
      error.value = report(e);
    } finally {
      busy.value = false;
    }
  }

  /**
   * 给加密包钥匙：口令 / 恢复码二选一，**两把任一把都能开**。
   *
   * 成功之后 `picked` 换成**解开的那个目录**——后面点「换库」时把它当普通来源递回去
   *（换库那条链一个字都没改）。失败时**不动 `picked`**（还指着那个加密包，作者能换一把钥匙再试），
   * 只把那句"打不开"摆出来。
   */
  async function unlock(kind: "passphrase" | "recovery", secret: string): Promise<void> {
    const source = picked.value;
    if (busy.value || !source) return;
    busy.value = true;
    try {
      const opened: UnsealedSource = await options.transport.unlock(source, kind, secret);
      preview.value = opened.preview;
      picked.value = opened.source;
      needKey.value = false;
      error.value = null;
    } catch (e) {
      error.value = report(e);
    } finally {
      busy.value = false;
    }
  }

  async function pickDatabase(title: string, filterLabel: string): Promise<void> {
    if (busy.value) return;
    busy.value = true;
    try {
      const source = await options.transport.pick(title, filterLabel);
      busy.value = false;
      if (source) await select(source);
    } catch (e) {
      error.value = report(e);
      busy.value = false;
    }
  }

  async function apply(): Promise<void> {
    const current = preview.value;
    if (busy.value || !picked.value || !current?.can_restore) return;
    busy.value = true;
    try {
      await options.beforeApply(); // 手上这一章先落盘（落不下就不换）
      done.value = await options.transport.apply(picked.value, tz());
      error.value = null;
      // 成功之后**故意**不收 busy：窗口马上重启，不该再让人点第二下
    } catch (e) {
      error.value = report(e);
      busy.value = false;
    }
  }

  return {
    visible,
    sources,
    picked,
    preview,
    busy,
    error,
    needKey,
    done,
    open: async () => {
      // 每次打开都重新扫一遍：备份位置可能刚插上/刚拔掉
      needKey.value = false; // 钥匙那一块跟着"选中项"一起清掉（换一份来源不该还摆着上一份的钥匙框）
      picked.value = null;
      preview.value = null;
      done.value = null;
      error.value = null;
      visible.value = true;
      await load();
    },
    close: () => {
      visible.value = false;
    },
    load,
    select,
    pickDatabase,
    unlock,
    apply,
  };
}
