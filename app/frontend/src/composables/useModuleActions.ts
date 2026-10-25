// 管理页里与单张卡有关的轻动作：激活/收藏/置顶/打开界面。
import type { Ref } from "vue";

import {
  moduleSetEnabled,
  moduleSetFavorite,
  moduleSetPinned,
  moduleUiOpen,
  moduleUninstall,
  moduleDataAudit,
  moduleDataClear,
  moduleDataExport,
  moduleDataIsolate,
  moduleDataRelease,
  moduleDataStats,
  type ModuleCard,
} from "../api/modules";
import { t } from "../locales/index.ts";

/** 体检里的一条事实 → 一句人话（认不出的码原样露出来，别静默吞掉）。 */
function issueLabel(code: string): string {
  return t(`modules.issue.${code}`);
}

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
      const state = stats.isolated
        ? t("modules.data_isolated_state", {
            reason: stats.isolation_reason || t("modules.isolation_no_reason"),
          })
        : stats.degraded
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

  /** 体检：只报事实与一句结论；**发现坏也不自动隔离**——那是作者的下一个决定。 */
  function auditData(card: ModuleCard) {
    act(async () => {
      const audit = await moduleDataAudit(card.id);
      const params = {
        tables: audit.tables,
        rows: audit.rows,
        integrity: t(`modules.integrity.${audit.integrity}`),
        count: audit.issues.length,
        list: audit.issues.map((issue) => issueLabel(issue.code)).join(t("common.list_separator")),
      };
      const head =
        audit.verdict === "ok"
          ? t("modules.data_audit_ok", params)
          : audit.verdict === "damaged"
            ? t("modules.data_audit_damaged", params)
            : t("modules.data_audit_attention", params);
      note.value =
        audit.stray_tables > 0
          ? head + t("modules.data_audit_stray", { count: audit.stray_tables })
          : head;
    });
  }

  function isolateData(card: ModuleCard) {
    if (!window.confirm(t("modules.isolate_confirm", { name: card.name }))) return;
    act(async () => {
      await moduleDataIsolate(card.id, t("modules.isolate_reason"));
      note.value = t("modules.isolated_done", { name: card.name });
    });
  }

  function releaseData(card: ModuleCard) {
    act(async () => {
      const was = await moduleDataRelease(card.id);
      note.value = was
        ? t("modules.released_done", { name: card.name })
        : t("modules.release_none", { name: card.name });
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

  return {
    toggleActive,
    toggleFavorite,
    togglePinned,
    openUi,
    dataSummary,
    auditData,
    isolateData,
    releaseData,
    exportData,
    clearData,
    uninstall,
  };
}
