// 「稿子放在哪」的状态与动作：首启确认位置、设置里换位置。
//
// # 分寸
//
// - **路径只在壳里**：这里拿到的 `picked.path` 只用来显示；真正搬家时界面只说一句"搬吧"，
//   不把路径递回壳——比"选备份来源"那条路更严（见 `commands::location`）；
// - **搬之前先落盘**：手上这一章先存下来，搬完重启才不会缺一段；
// - **风险只提示、不替作者决定**：同步盘/桌面/可移动盘都如实说；
//   真正危险的两种（往自己里面搬、目标已经有稿子）由壳拒绝，界面只负责把话说清。
//
// 传输是注入的：可以脱离界面与核心单测。

import { ref, type Ref } from "vue";

import type { LocationInfo, PickedDir, RelocationReport } from "../api/core";

/** 这个功能要用的几个动作（会话层注入真命令，测试注入替身）。 */
export interface LocationTransport {
  info: () => Promise<LocationInfo>;
  /** 让作者挑一个文件夹（返回 null = 取消了） */
  pick: (title: string) => Promise<PickedDir | null>;
  /** 首启确认「就用这里」 */
  confirm: () => Promise<void>;
  /** 搬到壳手里记着的那个位置 */
  move: () => Promise<RelocationReport>;
  /** 放弃这次选择 */
  cancel: () => Promise<void>;
}

export interface LocationOptions {
  transport: LocationTransport;
  /** 搬家之前先把手上这一章落盘；它失败就不该搬（丢字比搬不成严重得多） */
  beforeMove: () => Promise<void>;
  onError?: (message: string) => void;
}

export interface LocationState {
  /** 首启引导是否显示 */
  visible: Ref<boolean>;
  /** 现在的情况（没读回来之前是 null） */
  info: Ref<LocationInfo | null>;
  /** 刚选中的新位置（没选之前是 null） */
  picked: Ref<PickedDir | null>;
  busy: Ref<boolean>;
  error: Ref<string | null>;
  /** 搬完了（窗口马上重启，所以成功之后一直保持 busy） */
  moved: Ref<RelocationReport | null>;
  /** 读一次现状；壳说"第一次用"就把引导亮出来 */
  load: () => Promise<void>;
  /** 设置里主动打开"换位置" */
  open: () => Promise<void>;
  /** 收起面板（不动任何数据） */
  close: () => void;
  /** 挑一个文件夹（窗口标题由界面给，壳不产文案） */
  pickDir: (title: string) => Promise<void>;
  /** 首启选「就用这里」：记下位置并收起引导 */
  useCurrent: () => Promise<void>;
  /** 确认搬到刚选的位置 */
  confirmMove: () => Promise<void>;
  /** 不搬了（把壳里悬着的选择扔掉） */
  forget: () => Promise<void>;
}

export function useLocation(options: LocationOptions): LocationState {
  const visible = ref(false);
  const info = ref<LocationInfo | null>(null);
  const picked = ref<PickedDir | null>(null);
  const busy = ref(false);
  const error = ref<string | null>(null);
  const moved = ref<RelocationReport | null>(null);

  function fail(reason: unknown) {
    const message = reason instanceof Error ? reason.message : String(reason);
    error.value = message;
    options.onError?.(message);
  }

  async function refresh() {
    const current = await options.transport.info();
    info.value = current;
    return current;
  }

  async function load() {
    error.value = null;
    try {
      const current = await refresh();
      // 只有壳说"这是第一次用"才引导——老作者升上来不该看见"首次使用"
      if (current.first_run) visible.value = true;
    } catch (reason) {
      fail(reason);
    }
  }

  async function open() {
    error.value = null;
    try {
      await refresh();
      visible.value = true;
    } catch (reason) {
      fail(reason);
    }
  }

  function close() {
    visible.value = false;
    picked.value = null;
    error.value = null;
  }

  async function pickDir(title: string) {
    error.value = null;
    try {
      const chosen = await options.transport.pick(title);
      // 作者取消（壳回 null）：手里不留旧选择，免得下一次"确认"搬错地方
      picked.value = chosen;
    } catch (reason) {
      fail(reason);
    }
  }

  async function useCurrent() {
    busy.value = true;
    error.value = null;
    try {
      await options.transport.confirm();
      visible.value = false;
    } catch (reason) {
      fail(reason);
    } finally {
      busy.value = false;
    }
  }

  async function confirmMove() {
    if (!picked.value || busy.value) return;
    error.value = null;
    try {
      // **先落盘再搬家**：手上这一章存不下去就绝不搬
      await options.beforeMove();
    } catch (reason) {
      fail(reason);
      return;
    }
    busy.value = true;
    try {
      moved.value = await options.transport.move();
      // 成功后保持 busy：窗口马上重启，不留一个可以再按一遍的按钮
    } catch (reason) {
      busy.value = false;
      fail(reason);
    }
  }

  async function forget() {
    picked.value = null;
    error.value = null;
    try {
      await options.transport.cancel();
    } catch (reason) {
      fail(reason);
    }
  }

  return {
    visible,
    info,
    picked,
    busy,
    error,
    moved,
    load,
    open,
    close,
    pickDir,
    useCurrent,
    confirmMove,
    forget,
  };
}
