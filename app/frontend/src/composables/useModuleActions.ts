// 管理页里与单张卡有关的轻动作：激活/收藏/置顶/打开界面。
import type { Ref } from "vue";

import {
  moduleSetEnabled,
  moduleSetFavorite,
  moduleSetPinned,
  moduleUiOpen,
  moduleUninstall,
  moduleDataClear,
  moduleDataExport,
  moduleDataStats,
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

  function dataSummary(card: ModuleCard) {
    act(async () => {
      const stats = await moduleDataStats(card.id);
      const size =
        stats.db_bytes >= 1024 * 1024
          ? `${(stats.db_bytes / (1024 * 1024)).toFixed(1)} MB`
          : `${Math.ceil(stats.db_bytes / 1024)} KB`;
      const state = stats.degraded
        ? t("modules.data_degraded_state", { count: stats.rejected })
        : t("modules.data_ok_state");
      note.value = t("modules.data_summary", {
        tables: stats.tables,
        rows: stats.rows,
        size,
        state,
      });
    });
  }

  function exportData(card: ModuleCard) {
    act(async () => {
      const done = await moduleDataExport(card.id, t("modules.export_data"));
      note.value = done ? t("modules.data_exported") : t("modules.export_cancelled");
    });
  }

  function clearData(card: ModuleCard) {
    if (!window.confirm(t("modules.clear_data_confirm", { name: card.name }))) return;
    act(async () => {
      const count = await moduleDataClear(card.id);
      note.value = t("modules.data_cleared", { count });
    });
  }

  function uninstall(card: ModuleCard) {
    if (!window.confirm(t("modules.uninstall_confirm", { name: card.name }))) return;
    act(async () => {
      await moduleUninstall(card.id);
      note.value = t("modules.uninstalled", { name: card.name });
    });
  }

  function openUi(card: ModuleCard) {
    act(async () => {
      await moduleUiOpen(card.id);
      note.value = t("modules.ui_opened");
    });
  }

  return { toggleActive, toggleFavorite, togglePinned, openUi, dataSummary, exportData, clearData, uninstall };
}
