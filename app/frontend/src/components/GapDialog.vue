<script setup lang="ts">
// 删章路标那一问：**别猜，问一嘴**。
//
// 三条路都摆在明面上：补写（新建一个空章）/ 稍后再说（下次还问一次）/ 不用了（别再问）。
// 外加一条"去回收站看看"：他只是想先看一眼，**还没拿主意，所以不记答复**——下次点「+」还会问。
import type { ChapterGap } from "../api/core";
import { gapNote } from "../editor/gaps";
import { t } from "../locales/index.ts";

defineProps<{ gap: ChapterGap; busy: boolean }>();
defineEmits<{ fill: []; defer: []; ignore: []; trash: []; close: [] }>();
</script>

<template>
  <div class="gap dialog dialog--above" @click.self="$emit('close')">
    <section class="gap__box dialog__box">
      <h2 class="gap__title dialog__title">{{ t("gap.title") }}</h2>
      <p class="gap__note">{{ gapNote(gap) }}</p>
      <p class="gap__hint">
        {{ t("gap.fill_hint") }}<strong>{{ t("gap.fill_hint_strong") }}</strong>{{ t("gap.fill_hint_tail") }}
      </p>
      <div class="gap__actions">
        <button type="button" class="gap__button gap__button--primary dialog__button" :disabled="busy" @click="$emit('fill')">
          {{ t("gap.fill") }}
        </button>
        <button type="button" class="gap__button dialog__button" :disabled="busy" @click="$emit('defer')">
          {{ t("gap.defer") }}
        </button>
        <button type="button" class="gap__button dialog__button" :disabled="busy" @click="$emit('ignore')">
          {{ t("gap.ignore") }}
        </button>
        <button type="button" class="gap__button gap__button--ghost dialog__button" :disabled="busy" @click="$emit('trash')">
          {{ t("gap.trash") }}
        </button>
      </div>
    </section>
  </div>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./gap-dialog.css"></style>
