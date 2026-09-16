// 右侧第二栏现在露哪一块：**记住上次**（存在偏好里，跟人不跟书）。
//
// 单独成文件的理由与 `panels.ts` 一样：它是"当下露哪块"这件小事的状态，
// 而布局层那一份只管长什么样。与 `panels.ts` 的区别只有一条——
// 那个是专注模式里的浮层（**不落盘**），这个是常驻那一栏的分区，作者换过一次就该记住。
//
// 三条分寸：
// 1. **先动界面、再写偏好**：换页签是个即时动作，写库失败顶多下次忘了，不该卡住手；
// 2. **读回来之前用默认**：库里那份还没到就先露叩问（作者最常待的地方），
//    不猜一个"上次大概是创作流"；
// 3. **认不出的值当没设过**：与核心同一条规矩（核心那边本来也会把它挡掉）。

import { ref, watch, type Ref } from "vue";

import type { AppearanceState, AsideTab } from "./appearance.ts";

/** 没设过 / 读不出来时露哪一块（与核心的 `DEFAULT_ASIDE_TAB` 一致）。 */
export const DEFAULT_ASIDE_TAB: AsideTab = "flow";

export interface AsideState {
  /** 现在露哪一块 */
  tab: Ref<AsideTab>;
  /** 换一块（顺手写回偏好） */
  pick: (tab: AsideTab) => void;
}

export function useAsideTab(appearance: AppearanceState): AsideState {
  const tab = ref<AsideTab>(DEFAULT_ASIDE_TAB);
  // 库里那份读回来（或改完回读）之后跟着走：第一次读到就已经是"上次那一块"了。
  // `flush: "sync"`：偏好一变这一栏就跟着变，不留"晚一拍"的空窗——
  // 于是"读回来 → 露哪块"这件事是同步的、能直接单测，不必等一次 tick。
  watch(
    () => appearance.workValues.value?.aside_tab,
    (value) => {
      if (value === "flow" || value === "creator") tab.value = value;
    },
    { immediate: true, flush: "sync" },
  );
  return {
    tab,
    pick: (next) => {
      tab.value = next;
      void appearance.setAsideTab(next);
    },
  };
}
