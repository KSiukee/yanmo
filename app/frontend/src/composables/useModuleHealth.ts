// 管理页的**健康读数**：每个模块一行（上次成败 / 退避 / 隔离）。
//
// 只做"取数与拼句"，判断全在核心（留痕 / 退避账 / 隔离表）。
import { ref } from "vue";

import { moduleHealthList, type ModuleHealthView } from "../api/modules";
import { t } from "../locales/index.ts";

export function useModuleHealth() {
  const health = ref<Record<string, ModuleHealthView>>({});

  async function loadHealth() {
    try {
      const list = await moduleHealthList();
      health.value = Object.fromEntries(list.map((item) => [item.module_id, item]));
    } catch {
      health.value = {}; // 取不到就退成"还没跑过"，绝不挡列表
    }
  }

  /** 一行健康读数（没跑过 / 成功 / 失败 / 被断链拒 / 退避 / 隔离）。 */
  function healthLine(moduleId: string): string {
    const item = health.value[moduleId];
    if (!item || item.last_at === 0) return t("modules.health_never");
    const days = Math.floor((Date.now() - item.last_at) / 86_400_000);
    const when = days <= 0 ? t("modules.mirror_today") : t("display.days_ago", { days });
    const base =
      item.last_outcome === "ok"
        ? t("modules.health_ok", { when })
        : item.last_outcome === "cut"
          ? t("modules.health_cut", { when })
          : t("modules.health_failed", { code: item.last_code || "—", when });
    const backoff =
      item.backoff_until > Date.now()
        ? t("modules.health_backoff", {
            count: item.failures,
            seconds: Math.ceil((item.backoff_until - Date.now()) / 1000),
          })
        : "";
    const isolated = item.isolated ? t("modules.health_isolated") : "";
    return `${base}${backoff}${isolated}`;
  }

  return { health, loadHealth, healthLine };
}
