// 导入模块的三步（选包 → 预览与授权 → 确认 / 取消）单独一处。
//
// 为什么搬出来：管理页的 script 已经够长，而「挑包 / 预览 / 取消」是一条**完整的流程**
// （每一步都要先置忙、失败只改提示、成功才刷新列表）。搬出来之后，管理页只管布局与提示，
// 流程只看这一个文件。
import { ref, type Ref } from "vue";

import {
  moduleImportCancel,
  moduleImportConfirm,
  moduleImportPick,
  modulesList,
  type ImportPreview,
  type ModuleCapabilities,
  type ModuleCard,
} from "../api/modules";
import { t } from "../locales/index.ts";

/** 一条异常 → 界面上那句「为什么」的尾巴（不吞掉，原样带上）。 */
function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function useModuleImport(
  busy: Ref<boolean>,
  note: Ref<string>,
  onImported: (cards: ModuleCard[]) => void,
) {
  const preview = ref<ImportPreview | null>(null);

  async function pickImport() {
    if (busy.value) return;
    busy.value = true;
    note.value = "";
    try {
      const picked = await moduleImportPick(t("modules.import_pick_title"));
      if (!picked) return;
      preview.value = picked;
    } catch (error) {
      note.value = t("modules.import_failed", { detail: messageOf(error) });
    } finally {
      busy.value = false;
    }
  }

  async function confirmImport(grant: ModuleCapabilities) {
    const current = preview.value;
    if (!current || busy.value) return;
    busy.value = true;
    note.value = "";
    try {
      const card = await moduleImportConfirm(grant, current.fingerprint);
      preview.value = null;
      note.value = t("modules.import_ok", { name: card.name });
      onImported(await modulesList());
    } catch (error) {
      note.value = t("modules.import_failed", { detail: messageOf(error) });
    } finally {
      busy.value = false;
    }
  }

  async function cancelImport() {
    await moduleImportCancel().catch(() => undefined);
    preview.value = null;
  }

  return { preview, pickImport, confirmImport, cancelImport };
}
