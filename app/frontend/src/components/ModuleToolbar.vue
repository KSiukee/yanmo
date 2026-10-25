<script setup lang="ts">
// 管理页表头：全局离线 / 全部留镜像 / 导入 / 刷新 / 关闭。
//
// 只搬按钮与标签——动作全由管理页给（这里不碰数据、不判断任何东西）。
import { t } from "../locales/index.ts";

defineProps<{ busy: boolean; offline: boolean }>();
const emit = defineEmits<{
  offline: [];
  mirrorAll: [];
  import: [];
  refresh: [];
  close: [];
}>();
</script>

<template>
  <header class="modules__head dialog__head">
    <h2 class="modules__title dialog__title">{{ t("modules.title") }}</h2>
    <button
      type="button"
      class="modules__button dialog__button"
      :disabled="busy"
      @click="emit('offline')"
    >
      {{ offline ? t("modules.global_offline_on") : t("modules.global_offline_off") }}
    </button>
    <button
      type="button"
      class="modules__button dialog__button"
      :disabled="busy"
      @click="emit('mirrorAll')"
    >
      {{ t("modules.mirror_all_button") }}
    </button>
    <button
      type="button"
      class="modules__button dialog__button"
      :disabled="busy"
      @click="emit('import')"
    >
      {{ t("modules.import") }}
    </button>
    <button
      type="button"
      class="modules__button dialog__button"
      :disabled="busy"
      @click="emit('refresh')"
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
</template>

<style scoped src="./dialog.css"></style>
