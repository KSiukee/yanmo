// 创作留痕包：一键导出（对话框里只放按钮与结果，逻辑都在这儿）。
//
// 三条边界：
// - 壳让作者挑文件夹、写文件；核心只产字节；界面拿到的只有"写到哪儿了"；
// - 包里**不含正文**（过程材料与交稿是两件事）；
// - 取消 = 什么都不做，不弹错；失败就把原因如实摆出来（不吞）。
import { computed, ref } from "vue";

import { t } from "../locales/index.ts";
import type { EditorSession } from "../editor/session";
import { exportTracePackage } from "../api/core.ts";

export function useTraceExport(session: EditorSession) {
  const busy = ref(false);
  const note = ref("");
  /** 只收创作相关事件（滤掉性能诊断）；默认收全量 */
  const creativeOnly = ref(false);
  const workId = computed(() => session.workId.value);

  async function run(): Promise<void> {
    const work = session.workId.value;
    if (work === null) return;
    busy.value = true;
    note.value = "";
    try {
      const tz = -new Date().getTimezoneOffset();
      const result = await exportTracePackage(
        work,
        tz,
        t("backup.trace_pick_title"),
        creativeOnly.value ? "creative" : "all",
      );
      if (result !== null) {
        note.value = t("backup.trace_done", {
          dir: result.dir,
          events: String(result.events),
          head: `${result.head.slice(0, 12)}…`,
        });
      }
    } catch (error) {
      note.value = error instanceof Error ? error.message : String(error);
    } finally {
      busy.value = false;
    }
  }

  return { busy, note, workId, creativeOnly, run };
}
