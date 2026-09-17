<script setup lang="ts">
// 备份设置页：勾备份位置、订保留份数、看缺口、立即备份。
//
// 与别的弹窗同一条纪律：**这一页只显示与转发**——快照、体检、保留、账本都在核心。
// 只此一处说"备份成不成"：状态、原因、缺口全部来自核心给的现状与账本，界面不自己算。
import { computed, ref } from "vue";

import { t } from "../locales/index.ts";
import type { EditorSession } from "../editor/session";
import { formatBytes } from "../editor/display.ts";
import type { BackupConfig, BackupTarget } from "../api/core";

const props = defineProps<{ session: EditorSession }>();
const { status, lastReport, busy, error, close, save, runNow, dismissTip } =
  props.session.backup;

/** 保留份数（改了就存；输入非法时不动） */
const keepDraft = ref<number | null>(null);

const config = computed<BackupConfig | null>(() => status.value?.config ?? null);
/** 还没被勾上的盘（列出来给作者加） */
const availableVolumes = computed(() => {
  const current = status.value;
  if (!current) return [];
  const used = new Set(current.config.targets.map((target) => target.volume_id));
  return current.volumes.filter((volume) => !used.has(volume.volume_id));
});

/** 默认备份目录：`<盘根>研墨备份`——作者可以之后自己改。 */
function defaultPathFor(root: string): string {
  return `${root.replace(/[\\/]+$/, "")}\\研墨备份`;
}

function addVolume(volumeId: string): void {
  const current = status.value;
  if (!current) return;
  const volume = current.volumes.find((v) => v.volume_id === volumeId);
  if (!volume) return;
  const target: BackupTarget = {
    path: defaultPathFor(volume.root),
    volume_id: volume.volume_id,
    volume_label: volume.label || volume.root,
    removable: volume.removable,
  };
  void save({ ...current.config, targets: [...current.config.targets, target] });
}

function removeTarget(path: string): void {
  const current = status.value;
  if (!current) return;
  void save({ ...current.config, targets: current.config.targets.filter((x) => x.path !== path) });
}

function renameTarget(path: string, next: string): void {
  const current = status.value;
  if (!current) return;
  const trimmed = next.trim();
  if (!trimmed || trimmed === path) return;
  void save({
    ...current.config,
    targets: current.config.targets.map((x) => (x.path === path ? { ...x, path: trimmed } : x)),
  });
}

function setKeep(value: string): void {
  const current = status.value;
  if (!current) return;
  const parsed = Number.parseInt(value, 10);
  if (!Number.isFinite(parsed) || parsed < 1 || parsed > 60) return;
  keepDraft.value = parsed;
  void save({ ...current.config, keep: parsed });
}

function toggleAuto(key: "auto_on_start" | "auto_on_close"): void {
  const current = status.value;
  if (!current) return;
  void save({ ...current.config, [key]: !current.config[key] });
}

// 容量统一走 formatBytes（KB/MB/GB 一位小数）——**别在这里自己除**（上一版就是除错量纲，
// 把几十 GB 显示成「11 MB」，差点吓着人）
const sizeText = (bytes: number): string => formatBytes(bytes);

/** 从备份恢复：换到恢复页（先把这一页收起来，别两层弹窗叠着） */
function openRestore(): void {
  close();
  void props.session.restore.open();
}
</script>

<template>
  <div class="dialog" @click.self="close()">
    <section class="dialog__box backup">
      <header class="dialog__head">
        <h2 class="dialog__title">{{ t("backup.title") }}</h2>
        <button type="button" class="dialog__button" :title="t('backup.close_title')" @click="close()">
          {{ t("common.close") }}
        </button>
      </header>

      <p class="dialog__note">{{ t("backup.intro") }}</p>
      <p v-if="config && !config.targets.length" class="dialog__note dialog__note--warn">
        {{ t("backup.no_target") }}
      </p>
      <p v-else-if="status && !status.has_other_volume" class="dialog__note dialog__note--warn">
        {{ t("backup.advice_other_disk") }}
      </p>

      <!-- 已经指定的备份位置 -->
      <h3 class="backup__section">{{ t("backup.targets") }}</h3>
      <ul v-if="status" class="backup__list">
        <li v-for="target in status.targets" :key="target.path" class="backup__row">
          <div class="backup__main">
            <input
              class="backup__path"
              type="text"
              :value="target.path"
              @change="renameTarget(target.path, ($event.target as HTMLInputElement).value)"
            />
            <span class="backup__meta">
              {{ target.volume_label || "—" }} ·
              {{ target.last_success
                ? t("backup.target_last_success", { date: target.last_success })
                : t("backup.target_never") }}
              ·
              {{ target.gaps.length
                ? t("backup.target_gaps", { days: target.gaps.length })
                : t("backup.target_no_gaps") }}
            </span>
            <span v-if="!target.volume_present" class="backup__bad">{{ t("backup.target_missing_volume") }}</span>
            <span v-else-if="!target.dir_exists" class="backup__hint">{{ t("backup.target_no_dir") }}</span>
            <span v-else-if="target.last_problem" class="backup__bad">
              {{ t("backup.target_problem", { reason: target.last_problem }) }}
            </span>
          </div>
          <button type="button" class="dialog__button" @click="removeTarget(target.path)">
            {{ t("backup.remove") }}
          </button>
        </li>
      </ul>

      <!-- 能看到的盘：默认一个都不勾，只列出来给作者点 -->
      <h3 class="backup__section">{{ t("backup.volumes") }}</h3>
      <ul v-if="availableVolumes.length" class="backup__list">
        <li v-for="volume in availableVolumes" :key="volume.volume_id || volume.root" class="backup__row">
          <div class="backup__main">
            <span class="backup__label">{{ volume.root }}{{ volume.label ? ` ${volume.label}` : "" }}</span>
            <span class="backup__meta">
              <template v-if="sizeText(volume.free_bytes)">
                {{ t("backup.volume_free", { free: sizeText(volume.free_bytes) }) }} ·
              </template>
              <template v-if="volume.holds_data">{{ t("backup.volume_holds_data") }} · </template>
              <template v-if="volume.removable">{{ t("backup.volume_removable") }}</template>
            </span>
          </div>
          <button type="button" class="dialog__button" @click="addVolume(volume.volume_id)">
            {{ t("backup.add") }}
          </button>
        </li>
      </ul>
      <p v-else class="dialog__empty">{{ t("backup.no_volumes") }}</p>

      <!-- 保留与自动 -->
      <div v-if="config" class="backup__options">
        <label class="backup__option">
          {{ t("backup.keep") }}
          <input
            class="backup__number"
            type="number"
            min="1"
            max="60"
            :value="keepDraft ?? config.keep"
            @change="setKeep(($event.target as HTMLInputElement).value)"
          />
          <span class="backup__hint">{{ t("backup.keep_hint") }}</span>
        </label>
        <label class="backup__option">
          <input
            type="checkbox"
            :checked="config.auto_on_start"
            @change="toggleAuto('auto_on_start')"
          />
          {{ t("backup.auto_start") }}
        </label>
        <label class="backup__option">
          <input
            type="checkbox"
            :checked="config.auto_on_close"
            @change="toggleAuto('auto_on_close')"
          />
          {{ t("backup.auto_close") }}
        </label>
      </div>

      <div class="backup__actions">
        <button type="button" class="dialog__button dialog__button--primary" :disabled="busy" @click="runNow()">
          {{ busy ? t("backup.running") : t("backup.run") }}
        </button>
        <button type="button" class="dialog__button" :disabled="busy" @click="openRestore()">
          {{ t("backup.restore_entry") }}
        </button>
        <span v-if="status && !status.has_other_volume" class="backup__hint">
          <button type="button" class="backup__link" @click="dismissTip()">
            {{ t("backup.tip_dismiss") }}
          </button>
        </span>
      </div>

      <!-- 本次结果：逐目标成败，失败给人话原因 -->
      <template v-if="lastReport">
        <h3 class="backup__section">{{ t("backup.result") }}</h3>
        <ul class="backup__list">
          <li v-for="outcome in lastReport.outcomes" :key="outcome.path" class="backup__row">
            <div class="backup__main">
              <span class="backup__label">{{ outcome.path }}</span>
              <span class="backup__meta">
                {{ t(`backup.status.${outcome.status}`) }}
                <template v-if="outcome.status === 'written'">
                  · {{ t("backup.result_snapshot", { size: sizeText(outcome.bytes) }) }}
                  <template v-if="outcome.removed"> · {{ t("backup.result_removed", { count: outcome.removed }) }}</template>
                </template>
                <template v-else-if="outcome.reason"> · {{ outcome.reason }}</template>
              </span>
            </div>
          </li>
        </ul>
        <p v-if="lastReport.outcomes.some((o) => o.status === 'written')" class="dialog__note">
          {{ t("backup.safe_to_unplug") }}
        </p>
        <!-- 包写好了但账本没记上：必须说，否则「缺了哪几天」看着正常其实不准 -->
        <p v-if="lastReport.outcomes.some((o) => o.status === 'written') && !lastReport.ledger_written"
           class="backup__bad">
          {{ t("backup.ledger_not_written") }}
        </p>
      </template>

      <p v-if="error" class="backup__bad">{{ error }}</p>
    </section>
  </div>
</template>

<!-- 公共壳（遮罩 / 盒子 / 标题 / 按钮）：**必须引**，否则弹窗就是一堆裸内容叠在正文上 -->
<style scoped src="./dialog.css"></style>
<style scoped>
.backup {
  width: min(720px, 100%);
  overflow: auto;
}

.backup__section {
  margin: 14px 0 6px;
  font-size: 13px;
  font-weight: 600;
  color: var(--ym-ink-soft);
}

.backup__list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.backup__row {
  display: flex;
  align-items: center;
  gap: 8px;
  justify-content: space-between;
  padding: 6px 8px;
  border: 1px solid var(--ym-line);
  border-radius: 6px;
}

.backup__main {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.backup__path {
  min-width: 260px;
  font: inherit;
  padding: 2px 4px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: var(--ym-paper);
  color: inherit;
}

.backup__label {
  font-weight: 500;
}

.backup__meta {
  font-size: 12px;
  color: var(--ym-ink-soft);
}

.backup__bad {
  font-size: 12px;
  color: #b3261e;
}

.backup__options {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 14px;
}

.backup__option {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
}

.backup__number {
  width: 64px;
  font: inherit;
  padding: 2px 4px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: var(--ym-paper);
  color: inherit;
}

.backup__hint {
  font-size: 12px;
  color: var(--ym-ink-soft);
}

.backup__actions {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-top: 16px;
}

.backup__link {
  border: none;
  background: none;
  color: var(--ym-accent);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
  padding: 0;
}

.dialog__note--warn {
  color: #8a5a00;
}
</style>
