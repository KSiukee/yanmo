// 「设定」面板的**外壳状态**：开着没有、停在哪个页签。
//
// 单独成文件的原因：它**同时管两屏**（人物与设定 / 伏笔）——
// 可见性与页签不属于其中任何一屏，放在任何一边都会让另一边去 import 它。
//
// 三条分寸：
// 1. **打开才读**：它是"打开看一眼"的一屏，不是常驻画面（哪一屏活就读哪一屏）；
// 2. **打开时落到你要的那一页**：体检那条问题点"去设定卡改"或"去伏笔账本"，
//    打开的就得是那一页（这是"点得动"的一半）；
// 3. **收起时把手上的表单放下**：半填的表单不该在下次打开时冒出来。

import { ref, type Ref } from "vue";

import type { EntityPanelState } from "./entity-panel.ts";
import type { ForeshadowPanelState } from "./foreshadow-panel.ts";

/**
 * 「大纲」面板的五个页签（稳定码，界面上的字在字典 `lore.tab.*` 里）。
 *
 * 顺序就是"先有人、再有世界、再有发生了什么"：人物 → 设定 → 事件 → 伏笔。
 * （**场景卡那一页撤了**：它的四格进了「大纲」表——一章一行就地填，见 grid.ts。）
 */
export type LoreTab = "persons" | "settings" | "events" | "foreshadows";

export interface LoreOptions {
  entities: EntityPanelState;
  foreshadows: ForeshadowPanelState;
  /** 事件那一页用的是碎片池那一份状态（同一份数据，别读第二遍） */
  refreshEvents: () => Promise<void>;
  /** 切页签时把手上半填的表单放下（两屏共用的入口） */
  cancelForms: () => void;
}

export interface LoreState {
  visible: Ref<boolean>;
  tab: Ref<LoreTab>;
  /** 打开（可指定落到哪一页；不给就停在上一页） */
  show: (tab?: LoreTab) => void;
  hide: () => void;
  /** 换页签（顺手把那一屏读一次） */
  pick: (tab: LoreTab) => void;
}

export function useLore(options: LoreOptions): LoreState {
  const visible = ref(false);
  const tab = ref<LoreTab>("persons");

  /** 打开某一页就读那一页（不是五页一起读）。 */
  async function openPane(next: LoreTab) {
    if (next === "persons" || next === "settings") await options.entities.load();
    else if (next === "events") await options.refreshEvents();
    else await options.foreshadows.load();
  }

  return {
    visible,
    tab,
    show: (next) => {
      if (next) tab.value = next;
      visible.value = true;
      void openPane(tab.value);
    },
    hide: () => {
      visible.value = false;
      options.cancelForms();
    },
    pick: (next) => {
      tab.value = next;
      options.cancelForms();
      void openPane(next);
    },
  };
}
