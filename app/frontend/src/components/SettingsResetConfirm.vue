<script setup lang="ts">
// 「恢复默认」的确认层：**不可逆的全局动作，先问一句再动**。
//
// 单独一个组件有两个理由：
// 1. 设置面板本身别长胖（设置项只会越加越多，面板文件得留着地方）；
// 2. 边界清楚：这里只管"问一句"，真正的动作由父组件执行（confirmReset）。
import { t } from "../locales/index.ts";

defineProps<{ busy: boolean }>();
defineEmits<{ cancel: []; confirm: [] }>();
</script>

<template>
  <div class="settings dialog dialog--above" @click.self="$emit('cancel')">
    <section class="settings__box dialog__box">
      <header class="settings__head dialog__head">
        <h2 class="settings__title dialog__title">{{ t("settings.reset_confirm_title") }}</h2>
      </header>
      <p class="settings__hint">{{ t("settings.reset_confirm_body") }}</p>
      <div class="settings__actions">
        <button type="button" class="settings__button dialog__button" @click="$emit('cancel')">
          {{ t("common.cancel") }}
        </button>
        <button
          type="button"
          class="settings__button dialog__button settings__button--danger"
          :disabled="busy"
          @click="$emit('confirm')"
        >
          {{ t("settings.reset_confirm_ok") }}
        </button>
      </div>
    </section>
  </div>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./settings-dialog.css"></style>
