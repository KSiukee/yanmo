<script setup lang="ts">
// 删章路标那一问：**别猜，问一嘴**。
//
// 三条路都摆在明面上：补写（新建一个空章）/ 稍后再说（下次还问一次）/ 不用了（别再问）。
// 外加一条"去回收站看看"：他只是想先看一眼，**还没拿主意，所以不记答复**——下次点「+」还会问。
import type { ChapterGap } from "../api/core";
import { gapNote } from "../editor/gaps";

defineProps<{ gap: ChapterGap; busy: boolean }>();
defineEmits<{ fill: []; defer: []; ignore: []; trash: []; close: [] }>();
</script>

<template>
  <div class="gap" @click.self="$emit('close')">
    <section class="gap__box">
      <h2 class="gap__title">这一层少了一章</h2>
      <p class="gap__note">{{ gapNote(gap) }}</p>
      <p class="gap__hint">
        要在原来的位置上补写一个空章吗？——补写是<strong>新建一章</strong>，回收站里那份旧稿不会动。
      </p>
      <div class="gap__actions">
        <button type="button" class="gap__button gap__button--primary" :disabled="busy" @click="$emit('fill')">
          补写这一章
        </button>
        <button type="button" class="gap__button" :disabled="busy" @click="$emit('defer')">
          稍后再说
        </button>
        <button type="button" class="gap__button" :disabled="busy" @click="$emit('ignore')">
          不用了，别再问
        </button>
        <button type="button" class="gap__button gap__button--ghost" :disabled="busy" @click="$emit('trash')">
          去回收站看看
        </button>
      </div>
    </section>
  </div>
</template>

<style scoped src="./gap-dialog.css"></style>
