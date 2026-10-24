<script setup lang="ts">
// 模块管理页：**一个页面管所有模块**（收藏 / 置顶 / 激活冻结 / 卸载）。
//
// 这一版先做"看得见、管得住"：列表、排序偏好、激活/冻结、卸载（默认保留数据）。
// 后台运行 / 联网开关与启动按钮等宿主策略那一层接上再放出来——**不做假开关**。
import { onMounted, ref } from "vue";

import {
  moduleImportCancel,
  moduleImportConfirm,
  moduleImportPick,
  moduleSetEnabled,
  moduleSetFavorite,
  moduleSetPinned,
  moduleUninstall,
  modulesList,
  type ImportPreview,
  type ModuleCapabilities,
  type ModuleCard,
} from "../api/modules";
import ModuleImportPanel from "./ModuleImportPanel.vue";
import { t } from "../locales/index.ts";

const emit = defineEmits<{ close: [] }>();

const cards = ref<ModuleCard[]>([]);
const busy = ref(false);
const note = ref("");
const preview = ref<ImportPreview | null>(null);

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function readsLabel(card: ModuleCard): string {
  if (card.reads.length === 0) return t("modules.no_reads");
  return card.reads
    .map((code) => t(`modules.read.${code}`))
    .join(t("common.list_separator"));
}

async function load() {
  busy.value = true;
  try {
    cards.value = await modulesList();
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

function toggleActive(card: ModuleCard) {
  void act(() => moduleSetEnabled(card.id, !card.enabled));
}

function toggleFavorite(card: ModuleCard) {
  void act(() => moduleSetFavorite(card.id, !card.favorite));
}

function togglePinned(card: ModuleCard) {
  void act(() => moduleSetPinned(card.id, !card.pinned));
}

function uninstall(card: ModuleCard) {
  if (!window.confirm(t("modules.uninstall_confirm", { name: card.name }))) return;
  void act(async () => {
    await moduleUninstall(card.id);
    note.value = t("modules.uninstalled", { name: card.name });
  });
}

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
    cards.value = await modulesList();
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

onMounted(() => {
  void load();
});
</script>

<template>
  <div class="modules dialog" @click.self="emit('close')">
    <section class="modules__box dialog__box" role="dialog" aria-modal="true">
      <header class="modules__head dialog__head">
        <h2 class="modules__title dialog__title">{{ t("modules.title") }}</h2>
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
          @click="emit('close')"
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
              {{ card.certified ? t("modules.certified") : t("modules.uncertified") }}
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
          <p v-if="card.own_data || card.network" class="modules__caps">
            <span v-if="card.own_data" class="modules__badge">{{ t("modules.own_data") }}</span>
            <span v-if="card.network" class="modules__badge">{{ t("modules.network") }}</span>
          </p>
          <div class="modules__actions">
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
              type="button"
              class="modules__button dialog__button dialog__button--danger"
              :disabled="busy"
              @click="uninstall(card)"
            >
              {{ t("modules.uninstall") }}
            </button>
          </div>
        </li>
      </ul>
    </section>
  </div>
</template>

<style scoped src="./dialog.css"></style>

<style scoped>
.modules__box {
  width: min(760px, 100%);
}

.modules__list {
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.modules__card {
  padding: 10px 12px;
  border: 1px solid var(--ym-line);
  border-radius: 6px;
  background: var(--ym-paper);
}

.modules__card--frozen {
  opacity: 0.72;
}

.modules__row {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 8px;
}

.modules__name {
  font-size: 13px;
  font-weight: 600;
}

.modules__version,
.modules__meta,
.modules__desc,
.modules__reads {
  color: var(--ym-ink-soft);
  font-size: 12px;
}

.modules__meta,
.modules__desc,
.modules__reads,
.modules__caps {
  margin: 4px 0 0;
  overflow-wrap: anywhere;
}

.modules__caps {
  display: flex;
  gap: 6px;
}

.modules__badge {
  padding: 1px 6px;
  border-radius: 999px;
  background: var(--ym-paper-dim);
  color: var(--ym-ink-soft);
  font-size: 11px;
}

.modules__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 8px;
}

</style>
