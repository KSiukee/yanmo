// 专注模式下的悬浮卡片：两侧栏收起来之后，大纲这类"看一眼就走"的内容改成浮层。
//
// 三条分寸：
// 1. **卡片里放的是原来那个组件**（`DirectoryPane`），不写第二份——两份副本迟早长歪，
//    这也是"出现重复副本＝拆分硬信号"那条纪律的反面用法：能复用就别抄；
// 2. **一次只开一张**：同时压两张卡片，正文本就被浮层盖着，反而更乱；
//    点同一个按钮就是收起（不用记住"再点哪里关"）；
// 3. 它跟专注模式一样**不落盘**：当下这一会儿的状态，重开软件一律是收着的。

import { ref, type Ref } from "vue";

/** 能被唤出的卡片。用稳定代码当名字（界面上的字在 locales 里，代码不参与显示）。 */
export type PanelId = "outline";

export interface FloatingPanels {
  /** 现在开着哪一张（null = 都收着） */
  open: Ref<PanelId | null>;
  isOpen: (id: PanelId) => boolean;
  /** 唤起 / 收起同一个；返回收起后的状态（true = 现在是开着的） */
  toggle: (id: PanelId) => boolean;
  close: () => void;
}

export function useFloatingPanels(): FloatingPanels {
  const open = ref<PanelId | null>(null);
  return {
    open,
    isOpen: (id) => open.value === id,
    toggle: (id) => {
      open.value = open.value === id ? null : id;
      return open.value === id;
    },
    close: () => {
      open.value = null;
    },
  };
}
