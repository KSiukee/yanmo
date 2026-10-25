// 管理页里与单张卡有关的轻动作：激活/收藏/置顶/打开界面。
import type { Ref } from "vue";

import {
  moduleSetEnabled,
  moduleSetFavorite,
  moduleSetPinned,
  moduleUiOpen,
  type ModuleCard,
} from "../api/modules";
import { t } from "../locales/index.ts";

export function useModuleActions(
  note: Ref<string>,
  act: (action: () => Promise<void>) => void,
) {
  function toggleActive(card: ModuleCard) {
    void act(() => moduleSetEnabled(card.id, !card.enabled));
  }

  function toggleFavorite(card: ModuleCard) {
    act(() => moduleSetFavorite(card.id, !card.favorite));
  }

  function togglePinned(card: ModuleCard) {
    act(() => moduleSetPinned(card.id, !card.pinned));
  }

  function openUi(card: ModuleCard) {
    act(async () => {
      await moduleUiOpen(card.id);
      note.value = t("modules.ui_opened");
    });
  }

  return { toggleActive, toggleFavorite, togglePinned, openUi };
}
