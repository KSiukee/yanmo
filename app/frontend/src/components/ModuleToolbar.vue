<script setup lang="ts">
// 管理页表头：全局离线 / 全部留镜像 / 导入 / 刷新 / 关闭。
//
// 只搬按钮与标签——动作全由管理页给（这里不碰数据、不判断任何东西）。
import { t } from "../locales/index.ts";

defineProps<{ busy: boolean; offline: boolean }>();
const emit = defineEmits<{
  offline: [];
  panic: [];
  mirrorAll: [];
  restore: [];
  import: [];
  refresh: [];
  close: [];
}>();
</script>

<template>
  <header class="modules__head">
    <div class="modules__heading">
      <h2 class="modules__title">
        <svg class="modules__icon" viewBox="0 0 16 16" aria-hidden="true">
          <rect x="1.7" y="1.7" width="5.2" height="5.2" rx="1.1" />
          <rect x="9.1" y="1.7" width="5.2" height="5.2" rx="1.1" />
          <rect x="1.7" y="9.1" width="5.2" height="5.2" rx="1.1" />
          <rect x="9.1" y="9.1" width="5.2" height="5.2" rx="1.1" />
        </svg>
        {{ t("modules.title") }}
      </h2>
      <p class="modules__sub">{{ t("modules.dialog_sub") }}</p>
    </div>
    <div class="modules__tools">
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
        class="modules__button dialog__button dialog__button--danger"
        :disabled="busy"
        :title="t('modules.panic_title')"
        @click="emit('panic')"
      >
        {{ t("modules.panic_button") }}
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
        @click="emit('restore')"
      >
        {{ t("modules.restore_button") }}
      </button>
      <button
        type="button"
        class="modules__button dialog__button modules__button--primary"
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
    </div>
  </header>
  <!-- 关闭放在卡片右上角：它跟那排动作不是一类，挤在一起既难找又容易被挤到第二行。 -->
  <button
    type="button"
    class="modules__close"
    :title="t('modules.close_title')"
    @click="emit('close')"
  >
    ×
  </button>
</template>

<style scoped src="./dialog.css"></style>
