<script setup lang="ts">
// 半自动登记：模块把「正文里找到、但还不是设定卡」的名字提上来，作者点一下才落卡。
// 自己拉、自己轮询（模块跑完才会有新候选），管理页只把它摆进来。
import { computed, onMounted, onUnmounted, ref } from "vue";

import {
  moduleCandidates,
  moduleCandidatesApply,
  moduleQuestionsSubmit,
  type ModuleCandidate,
  type ModuleCandidatesView,
  type ModuleCard,
} from "../api/modules";
import { t } from "../locales/index.ts";

const props = defineProps<{ cards: ModuleCard[]; busy: boolean }>();
const emit = defineEmits<{ applied: [message: string] }>();

const views = ref<Record<string, ModuleCandidatesView>>({});
const dismissed = ref<Set<string>>(new Set());
let timer: number | undefined;

function nameOf(moduleId: string): string {
  return props.cards.find((card) => card.id === moduleId)?.name ?? moduleId;
}

const bars = computed(() =>
  Object.entries(views.value)
    .filter(([moduleId, view]) => view.items.length > 0 && !dismissed.value.has(moduleId))
    .map(([moduleId, view]) => ({ moduleId, view })),
);

function candidateText(candidate: ModuleCandidate): string {
  const chapters = candidate.chapters.map((index) => index + 1).join(t("common.list_separator"));
  return chapters
    ? t("modules.candidates_item", { name: candidate.name, chapters, count: candidate.count })
    : t("modules.candidates_item_no_chapter", { name: candidate.name, count: candidate.count });
}

async function load() {
  const next: Record<string, ModuleCandidatesView> = {};
  for (const card of props.cards) {
    try {
      const view = await moduleCandidates(card.id);
      if (view.items.length > 0) next[card.id] = view;
    } catch {
      // 读不到就不显示；下一轮再试
    }
  }
  views.value = next;
}

async function ask(moduleId: string) {
  try {
    const result = await moduleQuestionsSubmit(moduleId);
    emit(
      "applied",
      result.muted
        ? t("modules.questions_muted")
        : t("modules.questions_submitted", {
            accepted: result.accepted,
            dropped: result.dropped + result.skipped,
          }),
    );
  } catch (error) {
    emit(
      "applied",
      t("modules.action_failed", {
        detail: error instanceof Error ? error.message : String(error),
      }),
    );
  }
}

async function apply(moduleId: string) {
  const view = views.value[moduleId];
  if (!view) return;
  try {
    const names = view.items.map((item) => item.name);
    const created = await moduleCandidatesApply(moduleId, names);
    emit("applied", t("modules.candidates_done", { count: created }));
    dismissed.value.add(moduleId);
    await load();
  } catch (error) {
    emit(
      "applied",
      t("modules.action_failed", {
        detail: error instanceof Error ? error.message : String(error),
      }),
    );
  }
}

onMounted(() => {
  void load();
  timer = window.setInterval(() => {
    void load();
  }, 5000);
});

onUnmounted(() => {
  if (timer !== undefined) window.clearInterval(timer);
});
</script>

<template>
  <section v-if="bars.length > 0" class="modules__candidates">
    <p v-for="bar in bars" :key="bar.moduleId" class="modules__candidate">
      <span class="modules__candidate-text">
        {{
          t("modules.candidates_bar", {
            name: nameOf(bar.moduleId),
            count: bar.view.items.length,
            list: bar.view.items.map((item) => candidateText(item)).join(t("common.list_separator")),
          })
        }}
      </span>
      <button
        type="button"
        class="modules__button dialog__button"
        :disabled="busy"
        @click="ask(bar.moduleId)"
      >
        {{ t("modules.candidates_ask") }}
      </button>
      <button
        type="button"
        class="modules__button dialog__button"
        :disabled="busy"
        @click="apply(bar.moduleId)"
      >
        {{ t("modules.candidates_apply") }}
      </button>
      <button
        type="button"
        class="modules__button dialog__button"
        :disabled="busy"
        @click="dismissed.add(bar.moduleId)"
      >
        {{ t("modules.candidates_dismiss") }}
      </button>
    </p>
  </section>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./module-dialog.css"></style>
