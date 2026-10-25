<script setup lang="ts">
// 模块管理页：**一个页面管所有模块**（收藏 / 置顶 / 激活冻结 / 卸载）。
//
// 这一版先做"看得见、管得住"：列表、排序偏好、激活/冻结、卸载（默认保留数据）。
// 后台运行 / 联网开关与启动按钮等宿主策略那一层接上再放出来——**不做假开关**。
import { onMounted, onUnmounted, ref } from "vue";

import {
  moduleRun,
  moduleSetBackground,
  moduleStop,
  modulesList,
  type ModuleCard,
  type ModuleStatus,
} from "../api/modules";
import ModuleCredentials from "./ModuleCredentials.vue";
import ModuleImportPanel from "./ModuleImportPanel.vue";
import { certificationLabel, readsLabel } from "./moduleLabels";
import { useModuleActions } from "../composables/useModuleActions";
import { useModuleImport } from "../composables/useModuleImport";
import { useModuleNetwork } from "../composables/useModuleNetwork";
import { useModuleRuns } from "../composables/useModuleRuns";
import { t } from "../locales/index.ts";

const emit = defineEmits<{ close: [] }>();

const cards = ref<ModuleCard[]>([]);
const busy = ref(false);
const note = ref("");
const credentialFor = ref<string | null>(null);
const { runs, isRunning: isRunningId, markStarted, markStopped, loadStatuses, dispose } =
  useModuleRuns((status: ModuleStatus) => {
    const emitted = status.emitted.join(" / ");
    if (status.error) {
      note.value = t("modules.run_failed", { detail: status.error });
    } else if (status.data_degraded) {
      note.value = t("modules.data_degraded", { count: status.data_rejected });
    } else {
      note.value = status.timed_out
        ? t("modules.run_timeout")
        : emitted
          ? t("modules.run_ok", { emitted })
          : t("modules.run_ok_empty");
    }
  });

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

async function load() {
  busy.value = true;
  try {
    cards.value = await modulesList();
    await loadOffline();
    await loadStatuses(cards.value);
  } catch (error) {
    note.value = t("modules.load_failed", { detail: messageOf(error) });
  } finally {
    busy.value = false;
  }
}

async function act(action: () => Promise<void>) {
  if (busy.value) return;
  busy.value = true;
  note.value = "";
  try {
    await action();
    cards.value = await modulesList();
  } catch (error) {
    note.value = t("modules.action_failed", { detail: messageOf(error) });
  } finally {
    busy.value = false;
  }
}

const {
  offline,
  load: loadOffline,
  toggle: toggleOffline,
  toggleModule,
} = useModuleNetwork(note, act);
const {
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
  exportData,
  clearData,
  uninstall,
} = useModuleActions(note, act);

function toggleBackground(card: ModuleCard) {
  void act(async () => {
    await moduleSetBackground(card.id, !card.background);
    note.value = t(
      card.background ? "modules.background_disabled" : "modules.background_enabled",
      { name: card.name },
    );
  });
}

function closeDialog() {
  // 没开后台运行的，离开管理页就停；开了的继续跑（仍可再打开停）。
  for (const [moduleId, status] of Object.entries(runs.value)) {
    if (!status.running) continue;
    const card = cards.value.find((item) => item.id === moduleId);
    if (!card?.background) {
      void moduleStop(moduleId);
    }
  }
  dispose();
  emit("close");
}

function runModule(card: ModuleCard) {
  void act(async () => {
    await moduleRun(card.id);
    markStarted(card.id);
    note.value = t("modules.run_started", { name: card.name });
  });
}

function stopModule(card: ModuleCard) {
  void act(async () => {
    await moduleStop(card.id);
    note.value = t("modules.run_stopped");
    markStopped(card.id);
  });
}

function isRunning(card: ModuleCard): boolean {
  return isRunningId(card.id);
}

// 导入流程（挑包 / 预览 / 确认 / 取消）在 useModuleImport 里；这里只把结果贴回列表。
const {
  preview,
  pickImport,
  confirmImport,
  cancelImport,
} = useModuleImport(busy, note, (next) => {
  cards.value = next;
});

onMounted(() => {
  void load();
});

onUnmounted(() => {
  dispose();
});
</script>

<template>
  <div class="modules dialog" @click.self="closeDialog">
    <section class="modules__box dialog__box" role="dialog" aria-modal="true">
      <header class="modules__head dialog__head">
        <h2 class="modules__title dialog__title">{{ t("modules.title") }}</h2>
        <button
          type="button"
          class="modules__button dialog__button"
          :disabled="busy"
          @click="toggleOffline"
        >
          {{
            offline ? t("modules.global_offline_on") : t("modules.global_offline_off")
          }}
        </button>
        <button
          type="button"
          class="modules__button dialog__button"
          :disabled="busy"
          @click="pickImport"
        >
          {{ t("modules.import") }}
        </button>
        <button
          type="button"
          class="modules__button dialog__button"
          :disabled="busy"
          @click="load"
        >
          {{ t("modules.refresh") }}
        </button>
        <button
          type="button"
          class="modules__button dialog__button"
          :title="t('modules.close_title')"
          @click="closeDialog"
        >
          {{ t("common.close") }}
        </button>
      </header>

      <p v-if="note" class="modules__note dialog__note">{{ note }}</p>

      <ModuleImportPanel
        v-if="preview"
        :key="preview.fingerprint"
        :preview="preview"
        :busy="busy"
        @confirm="confirmImport"
        @cancel="cancelImport"
      />

      <p v-if="cards.length === 0" class="modules__empty dialog__empty">
        {{ t("modules.empty") }}
      </p>

      <ul v-else class="modules__list">
        <li
          v-for="card in cards"
          :key="card.id"
          class="modules__card"
          :class="{ 'modules__card--frozen': !card.enabled }"
        >
          <div class="modules__row">
            <span class="modules__name">{{ card.name }}</span>
            <span class="modules__version">v{{ card.version }}</span>
            <span class="modules__badge">
              {{ certificationLabel(card) }}
            </span>
            <span class="modules__badge">
              {{ card.enabled ? t("modules.active") : t("modules.frozen") }}
            </span>
          </div>
          <p class="modules__meta">
            {{ t("modules.author", { author: card.author }) }} · {{ card.id }}
          </p>
          <p v-if="card.description" class="modules__desc">{{ card.description }}</p>
          <p class="modules__reads">{{ t("modules.reads", { reads: readsLabel(card) }) }}</p>
          <p v-if="card.fingerprint" class="modules__reads">
            {{
              t("modules.import_fingerprint", {
                fingerprint: `${card.fingerprint.slice(0, 16)}…`,
              })
            }}
          </p>
          <p v-if="runs[card.id]?.running" class="modules__reads">
            {{ t("modules.run_health", { idle: runs[card.id]?.idle_seconds ?? 0 }) }}
          </p>
          <p v-if="card.own_data || card.network" class="modules__caps">
            <span v-if="card.own_data" class="modules__badge">{{ t("modules.own_data") }}</span>
            <span v-if="card.network" class="modules__badge">{{ t("modules.network") }}</span>
          </p>
          <div class="modules__actions">
            <button v-if="card.own_data" type="button" class="modules__button dialog__button" :disabled="busy" @click="dataSummary(card)">{{ t("modules.data_summary_button") }}</button>
            <button v-if="card.own_data" type="button" class="modules__button dialog__button" :disabled="busy" @click="auditData(card)">{{ t("modules.data_audit_button") }}</button>
            <button v-if="card.own_data" type="button" class="modules__button dialog__button" :disabled="busy" @click="isolateData(card)">{{ t("modules.isolate_button") }}</button>
            <button v-if="card.own_data" type="button" class="modules__button dialog__button" :disabled="busy" @click="releaseData(card)">{{ t("modules.release_button") }}</button>
            <button v-if="card.own_data" type="button" class="modules__button dialog__button" :disabled="busy" @click="mirrorData(card)">{{ t("modules.mirror_button") }}</button>
            <button v-if="card.own_data" type="button" class="modules__button dialog__button" :disabled="busy" @click="rebuildData(card)">{{ t("modules.rebuild_button") }}</button>
            <button v-if="card.own_data" type="button" class="modules__button dialog__button" :disabled="busy" @click="exportData(card)">{{ t("modules.export_data") }}</button>
            <button v-if="card.own_data" type="button" class="modules__button dialog__button" :disabled="busy" @click="clearData(card)">{{ t("modules.clear_data") }}</button>
            <button
              v-if="isRunning(card)"
              type="button"
              class="modules__button dialog__button"
              :disabled="busy"
              @click="runModule(card)"
            >
              {{ t("modules.run_again") }}
            </button>
            <button
              v-if="card.ui_form === 'embedded'"
              type="button"
              class="modules__button dialog__button"
              :disabled="busy"
              @click="openUi(card)"
            >
              {{ t("modules.open_ui") }}
            </button>
            <button
              v-if="isRunning(card)"
              type="button"
              class="modules__button dialog__button"
              :disabled="busy"
              @click="stopModule(card)"
            >
              {{ t("modules.stop") }}
            </button>
            <button
              v-else
              type="button"
              class="modules__button dialog__button"
              :disabled="busy || !card.enabled"
              @click="runModule(card)"
            >
              {{ t("modules.run") }}
            </button>
            <button
              type="button"
              class="modules__button dialog__button"
              :disabled="busy"
              @click="toggleActive(card)"
            >
              {{ card.enabled ? t("modules.freeze") : t("modules.activate") }}
            </button>
            <button
              type="button"
              class="modules__button dialog__button"
              :disabled="busy"
              @click="toggleFavorite(card)"
            >
              {{ card.favorite ? t("modules.unfavorite") : t("modules.favorite") }}
            </button>
            <button
              type="button"
              class="modules__button dialog__button"
              :disabled="busy"
              @click="togglePinned(card)"
            >
              {{ card.pinned ? t("modules.unpin") : t("modules.pin") }}
            </button>
            <button
              v-if="card.network_declared"
              type="button"
              class="modules__button dialog__button"
              :disabled="busy"
              @click="toggleModule(card)"
            >
              {{
                card.network ? t("modules.network_on") : t("modules.network_off")
              }}
            </button>
            <button
              v-if="card.network_declared"
              type="button"
              class="modules__button dialog__button"
              :disabled="busy"
              @click="credentialFor = credentialFor === card.id ? null : card.id"
            >
              {{ t("credentials.button") }}
            </button>
            <button
              type="button"
              class="modules__button dialog__button"
              :disabled="busy"
              @click="toggleBackground(card)"
            >
              {{
                card.background
                  ? t("modules.background_on")
                  : t("modules.background_off")
              }}
            </button>
            <button
              type="button"
              class="modules__button dialog__button dialog__button--danger"
              :disabled="busy"
              @click="uninstall(card)"
            >
              {{ t("modules.uninstall") }}
            </button>
          </div>
          <ModuleCredentials
            v-if="credentialFor === card.id"
            :module-id="card.id"
            @close="credentialFor = null"
          />
        </li>
      </ul>
    </section>
  </div>
</template>

<style scoped src="./dialog.css"></style>

<style scoped src="./module-dialog.css"></style>
