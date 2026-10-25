//! 管理页的**全局离线**与模块联网开关：只碰设置，不碰运行状态。
import { ref, type Ref } from "vue";

import { moduleSetNetwork, networkOffline, networkSetOffline, type ModuleCard } from "../api/modules";
import { t } from "../locales/index.ts";

/** `act` 由管理页给：统一 busy/错误提示/刷新卡片。 */
export function useModuleNetwork(
  note: Ref<string>,
  act: (action: () => Promise<void>) => void,
) {
  const offline = ref(false);

  async function load() {
    offline.value = await networkOffline();
  }

  function toggleModule(card: ModuleCard) {
    if (!card.network && !window.confirm(t("modules.network_confirm"))) return;
    act(async () => {
      await moduleSetNetwork(card.id, !card.network);
      note.value = t(
        card.network ? "modules.network_off_done" : "modules.network_on_done",
        { name: card.name },
      );
    });
  }

  function toggle() {
    if (!offline.value && !window.confirm(t("modules.global_offline_confirm"))) return;
    act(async () => {
      offline.value = await networkSetOffline(!offline.value);
      note.value = offline.value
        ? t("modules.global_offline_on_done")
        : t("modules.global_offline_off_done");
    });
  }

  return { offline, load, toggle, toggleModule };
}
