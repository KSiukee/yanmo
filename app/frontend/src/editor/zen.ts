// 专注模式：**只留正文**的无干扰写作态。
//
// 三条分寸：
// 1. 它只管"看不看得见"，**不碰数据、不改正文一个字**——退出后跟进去之前一模一样；
// 2. 它是"当下这一会儿"的状态，**不落盘**：重开软件回到常规三栏（记住反而会让人下次找不到侧栏）；
// 3. "露哪几块"的判断放在这里（视图只照着渲染，不自己 if），所以能脱离界面单测。

import { computed, ref, type ComputedRef, type Ref } from "vue";

/** 专注态下界面各部分露不露——视图照这个渲染，不自己判断。 */
export interface ZenChrome {
  /** 左侧目录栏 */
  directory: boolean;
  /** 右侧叩问栏 */
  flow: boolean;
  /** 顶部那一条（品牌 + 书架 / 设置 / 备份 + 章名） */
  topbar: boolean;
  /** 编辑器自带那条细条（章名 / 字数 / 今日进度 / 保存状态）——专注时**留着**，退出按钮也在上面 */
  editorBar: boolean;
}

/** 常规三栏：该露的都露。 */
export const ZEN_OFF: ZenChrome = {
  directory: true,
  flow: true,
  topbar: true,
  editorBar: true,
};

/**
 * 专注：藏两侧栏与顶栏，只留正文 + 编辑器那条细状态栏。
 *
 * 为什么留 `editorBar`：写字的时候作者最需要的就是它——字数、今日进度、存没存上，
 * 全在那一条上；把它也藏了，专注就变成"什么都不知道了"。
 */
export const ZEN_ON: ZenChrome = {
  directory: false,
  flow: false,
  topbar: false,
  editorBar: true,
};

export function zenChrome(on: boolean): ZenChrome {
  return on ? ZEN_ON : ZEN_OFF;
}

export interface ZenState {
  on: Ref<boolean>;
  /** 界面该露哪几块（视图直接绑它） */
  chrome: ComputedRef<ZenChrome>;
  enter: () => void;
  exit: () => void;
  /** 切换；返回切换后的状态（快捷键层据此决定要不要连全屏一起收） */
  toggle: () => boolean;
}

export function useZen(): ZenState {
  const on = ref(false);
  return {
    on,
    chrome: computed(() => zenChrome(on.value)),
    enter: () => {
      on.value = true;
    },
    exit: () => {
      on.value = false;
    },
    toggle: () => {
      on.value = !on.value;
      return on.value;
    },
  };
}
