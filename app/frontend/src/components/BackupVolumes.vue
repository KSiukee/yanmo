<script setup lang="ts">
// 备份面板的「盘」区：能看到的盘 + 「该用哪块盘」的建议（并当场加为目标）。
//
// 单独成件：这一块是**一整块界面**（建议条 + 盘列表 + 一键加），把它从
// BackupDialog 里搬出来，那一页才不至于贴着行数上限长。
import { computed } from "vue";

import { t } from "../locales/index.ts";
import { formatBytes } from "../editor/display.ts";
import type { BackupVolume } from "../api/core";
import { recommendVolume } from "../editor/backup-advice.ts";

const props = defineProps<{
  /** 还没被勾成目标的盘 */
  volumes: BackupVolume[];
  /** 该不该给「用哪块盘」的建议（目标全在数据所在盘上时给） */
  showAdvice: boolean;
}>();

const emit = defineEmits<{ add: [volumeId: string] }>();

/** 推荐用哪块盘（可移动优先、跳过数据所在盘、剩余空间要够）；没有合适的为 null */
const advice = computed(() => recommendVolume(props.volumes));

const sizeText = (bytes: number): string => formatBytes(bytes);

/** 一块盘在界面上怎么称呼（盘符 + 卷标）。 */
function volumeName(volume: BackupVolume): string {
  return `${volume.root}${volume.label ? ` ${volume.label}` : ""}`;
}
</script>

<template>
  <!-- 建议条：只有"该建议"且真有合适的盘时才出现；没有合适的盘就退回原来那句 -->
  <p v-if="showAdvice && advice" class="dialog__note dialog__note--warn">
    {{
      advice.reason === "removable"
        ? t("backup.advice_use_removable", {
            volume: volumeName(advice.volume),
            free: sizeText(advice.volume.free_bytes),
          })
        : t("backup.advice_use_roomy", {
            volume: volumeName(advice.volume),
            free: sizeText(advice.volume.free_bytes),
          })
    }}
    <button type="button" class="dialog__button" @click="emit('add', advice.volume.volume_id)">
      {{ t("backup.advice_use_now") }}
    </button>
  </p>
  <p v-else-if="showAdvice" class="dialog__note dialog__note--warn">{{ t("backup.advice_other_disk") }}</p>

  <!-- 能看到的盘：默认一个都不勾，只列出来给作者点 -->
  <h3 class="backup__section">{{ t("backup.volumes") }}</h3>
  <ul v-if="volumes.length" class="backup__list">
    <li v-for="volume in volumes" :key="volume.volume_id || volume.root" class="backup__row">
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
      <button type="button" class="dialog__button" @click="emit('add', volume.volume_id)">
        {{ t("backup.add") }}
      </button>
    </li>
  </ul>
  <p v-else class="dialog__empty">{{ t("backup.no_volumes") }}</p>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./backup-pane.css"></style>
