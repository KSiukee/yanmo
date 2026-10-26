<script setup lang="ts">
// 管理页里"等作者点头"的那几排：读数据请示 / 候选 / 建议 / 词表。
// 只做竖排与转发，不含任何业务判断（判断都在各自的组件与核心）。
import type { ModuleCard } from "../api/modules";
import ModuleCandidates from "./ModuleCandidates.vue";
import ModuleProposals from "./ModuleProposals.vue";
import ModuleReadAsks from "./ModuleReadAsks.vue";
import ModuleWords from "./ModuleWords.vue";

defineProps<{ cards: ModuleCard[]; busy: boolean }>();
const emit = defineEmits<{ done: [message: string] }>();
</script>

<template>
  <ModuleReadAsks :cards="cards" :busy="busy" @answered="emit('done', $event)" />
  <ModuleCandidates :cards="cards" :busy="busy" @applied="emit('done', $event)" />
  <ModuleProposals :cards="cards" :busy="busy" @done="emit('done', $event)" />
  <ModuleWords
    v-if="cards.length === 1"
    :module-id="cards[0].id"
    :busy="busy"
    @done="emit('done', $event)"
  />
</template>
