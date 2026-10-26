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
  moduleDataMirror,
  moduleDataMirrorAll,
  moduleDataRebuild,
  moduleDataRelease,
  moduleDataStats,
  moduleRestoreAll,
  moduleRestorePick,
  type ModuleCard,
} from "../api/modules";
import { t } from "../locales/index.ts";

/** 体检里的一条事实 → 一句人话（认不出的码原样露出来，别静默吞掉）。 */
function issueLabel(code: string): string {
  return t(`modules.issue.${code}`);
}

/** 字节数 → 人读的大小（与「数据概况」同一口径）。 */
function sizeText(bytes: number): string {
  return bytes >= 1024 * 1024
    ? `${(bytes / (1024 * 1024)).toFixed(1)} MB`
    : `${Math.ceil(bytes / 1024)} KB`;
}

/** 毫秒时间戳 → 「今天 / N 天前」。 */
function whenText(at: number): string {
  const days = Math.floor((Date.now() - at) / 86_400_000);
  return days <= 0 ? t("modules.mirror_today") : t("display.days_ago", { days });
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
        size: sizeText(stats.db_bytes),
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
      const receipt = await moduleDataIsolate(card.id, t("modules.isolate_reason"));
      note.value = receipt.mirrored
        ? t("modules.isolated_done", { name: card.name })
        : t("modules.isolated_without_mirror", {
            name: card.name,
            detail: receipt.mirror_error ?? "",
          });
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

  /** 写镜像：先把旧镜像与主库核对一遍（说清差在哪），再写新的。 */
  function mirrorData(card: ModuleCard) {
    act(async () => {
      const refresh = await moduleDataMirror(card.id);
      const before = refresh.before_error
        ? t("modules.mirror_before_unreadable", { detail: refresh.before_error })
        : refresh.before
          ? refresh.before.state === "ok"
            ? t("modules.mirror_before_ok", { when: whenText(refresh.before.written_at) })
            : t("modules.mirror_before_different", {
                when: whenText(refresh.before.written_at),
                count: refresh.before.differences.length,
              })
          : t("modules.mirror_before_missing");
      const written = t("modules.mirror_done", {
        tables: refresh.written.tables,
        rows: refresh.written.rows,
        size: sizeText(refresh.written.bytes),
      });
      note.value = before + written;
    });
  }

  /** 用镜像重建：**只在这一格被隔离时**核心才放行（界面先问一句，拒绝留给核心说）。 */
  function rebuildData(card: ModuleCard) {
    if (!window.confirm(t("modules.rebuild_confirm", { name: card.name }))) return;
    act(async () => {
      const rebuilt = await moduleDataRebuild(card.id);
      note.value =
        rebuilt.schema_version === null
          ? t("modules.rebuild_done", { tables: rebuilt.tables, rows: rebuilt.rows })
          : t("modules.rebuild_done_schema", {
              tables: rebuilt.tables,
              rows: rebuilt.rows,
              schema: rebuilt.schema_version,
            });
    });
  }

  /** 全部留镜像：一次把所有有账的模块各写一份（收工前的「留底」）。 */
  function mirrorAll() {
    act(async () => {
      const receipt = await moduleDataMirrorAll();
      note.value =
        receipt.modules === 0
          ? t("modules.mirror_all_empty")
          : t("modules.mirror_all_done", {
              modules: receipt.modules,
              tables: receipt.tables,
              rows: receipt.rows,
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

  /** 整库恢复：选备份目录 → 搬镜像 → 逐模块重建（结果一行话说清）。 */
  function restoreAll(reload: () => Promise<void>) {
    act(async () => {
      const from = await moduleRestorePick(t("modules.restore_pick"));
      if (!from) {
        note.value = t("modules.restore_none");
        return;
      }
      const report = await moduleRestoreAll(from);
      const ok = report.items.filter((item) => item.ok).length;
      note.value = t("modules.restore_done", {
        found: report.found,
        ok,
        failed: report.items.length - ok,
      });
      await reload();
    });
  }

  return {
    restoreAll,
    toggleActive,
    toggleFavorite,
    togglePinned,
    openUi,
    dataSummary,
    auditData,
    isolateData,
    releaseData,
    mirrorData,
    rebuildData,
    mirrorAll,
    exportData,
    clearData,
    uninstall,
  };
}
