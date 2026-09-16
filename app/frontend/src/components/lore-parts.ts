// 「大纲」这一族（人物与设定 / 事件 / 场景卡 / 伏笔）的**一处装配**。
//
// 为什么单独成件：它们五页是一个整体（面板把五份状态摆到五片页签上），
// 而"每加一页就在会话的装配函数里多写十行"会让那个本来就很长的函数继续长
// （会话是**唯一装配处**，那里只该剩"把这一族接上"这一句）。
//
// 两份状态各自成件（entities / foreshadows），事件那页复用**创作流那一份**
// 碎片状态——同一份数据读两遍，迟早对不上。

import type { Ref } from "vue";

import { useCreatorPanel, type CreatorPanelState } from "./creator-panel.ts";
import { useEntityPanel, type EntityPanelState } from "./entity-panel.ts";
import { useForeshadowPanel, type ForeshadowPanelState } from "./foreshadow-panel.ts";
import { useLore, type LoreState } from "./lore.ts";

export interface LoreParts {
  /** 人物与设定（两页共用一份名单） */
  entities: EntityPanelState;
  /** 伏笔 */
  foreshadows: ForeshadowPanelState;
  /** 事件那页用的是创作流那一份碎片状态（两处共用） */
  creator: CreatorPanelState;
  /** 面板外壳：开着没有、停在哪个页签 */
  lore: LoreState;
}

export interface LorePartsOptions {
  workId: Ref<number | null>;
  /** "当前这一章"：记伏笔 / 记事件的锚点、新建场景卡挂在它下面 */
  currentChapter: Ref<number | null>;
}

export function useLoreParts(deps: LorePartsOptions): LoreParts {
  const entities = useEntityPanel({ workId: deps.workId });
  const foreshadows = useForeshadowPanel({ workId: deps.workId, currentChapter: deps.currentChapter });
  const creator = useCreatorPanel({ workId: deps.workId, currentChapter: deps.currentChapter });
  const lore = useLore({
    entities,
    foreshadows,
    refreshEvents: async () => {
      await creator.refresh();
    },
    cancelForms: () => {
      entities.cancelEdit();
      foreshadows.cancelEdit();
    },
  });
  return { entities, foreshadows, creator, lore };
}
