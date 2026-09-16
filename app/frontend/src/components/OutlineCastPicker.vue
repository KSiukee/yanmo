<script setup lang="ts">
// 「出场人物」那一格的多选卡：**勾谁出场**（点一下就落库）。
//
// 单独成件的原因：它是表里唯一"点选"的一格——与打字、拖动那两套手感都不搭。
// 留在大纲表那个组件里，只会让那个本来就很长的文件再长一截（0.68.0 的拆分）。
//
// 两件事由外面管（这里只画）：名单是谁（`selected`）、有哪些卡可选（`cards`）；
// 勾一下与收起来都发事件出去，**自己不猜、不存**。
import type { EntityCard } from "../api/entity.ts";
import type { CastMember } from "../api/outline.ts";
import { t } from "../locales/index.ts";

defineProps<{
  /** 这本书的人物卡（空＝还没建过人物卡） */
  cards: EntityCard[];
  /** 这一段已经挂着的人 */
  selected: CastMember[];
  /** 忙的时候不让勾：两次快速点击会各自按手上那份旧名单算，后一次把前一次盖掉 */
  busy: boolean;
}>();

const emit = defineEmits<{
  toggle: [entity_id: number];
  close: [];
}>();
</script>

<template>
  <div class="grid__cast-pop">
    <p class="grid__cast-hint">
      {{ cards.length === 0 ? t("grid.cast.no_cards") : t("grid.cast.hint") }}
    </p>
    <label v-for="card in cards" :key="card.id" class="grid__cast-item">
      <input
        type="checkbox"
        :checked="selected.some((member) => member.entity_id === card.id)"
        :disabled="busy"
        @change="emit('toggle', card.id)"
      />
      <span>{{ card.name }}</span>
    </label>
    <button type="button" class="grid__cast-done" @click="emit('close')">
      {{ t("common.close") }}
    </button>
  </div>
</template>

<style scoped src="./outline-cast.css"></style>
