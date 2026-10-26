<script setup lang="ts">
// 模块提的正文建议：作者审，点一下才落盘。失效的（基于旧版本）要标出来，不许静默套用。
import { computed, onMounted, onUnmounted, ref } from "vue";

import {
  moduleProposalApply,
  moduleProposalDismiss,
  moduleProposals,
  type ModuleCard,
  type ProposalView,
} from "../api/modules";
import { t } from "../locales/index.ts";

const props = defineProps<{ cards: ModuleCard[]; busy: boolean }>();
const emit = defineEmits<{ done: [message: string] }>();

const items = ref<Record<string, ProposalView[]>>({});
const suppressed = ref<Record<string, number>>({});
// 额度提示也要能显示（哪怕一条都没摆出来）
const capped = computed(() =>
  Object.entries(suppressed.value)
    .filter(([, count]) => count > 0)
    .map(([moduleId, count]) => ({ moduleId, count })),
);
let timer: number | undefined;

function nameOf(moduleId: string): string {
  return props.cards.find((card) => card.id === moduleId)?.name ?? moduleId;
}

const bars = computed(() =>
  Object.entries(items.value)
    .filter(([, list]) => list.length > 0)
    .map(([moduleId, list]) => ({ moduleId, list })),
);

async function load() {
  const next: Record<string, ProposalView[]> = {};
  for (const card of props.cards) {
    try {
      const view = await moduleProposals(card.id);
      if (view.items.length > 0 || view.suppressed > 0) {
        next[card.id] = view.items;
        if (view.suppressed > 0) {
          suppressed.value[card.id] = view.suppressed;
        }
      }
    } catch {
      // 读不到就不显示
    }
  }
  items.value = next;
}

async function apply(moduleId: string, proposal: ProposalView) {
  try {
    const outcome = await moduleProposalApply(moduleId, proposal.id);
    emit(
      "done",
      outcome.status === "applied"
        ? t("modules.proposal_applied", { chars: outcome.chars ?? 0 })
        : t("modules.proposal_stale"),
    );
  } catch (error) {
    emit("done", t("modules.action_failed", { detail: String(error) }));
  }
  await load();
}

async function applyAll(moduleId: string) {
  const list = (items.value[moduleId] ?? []).filter((item) => !item.stale);
  if (list.length === 0) return;
  let applied = 0;
  for (const item of list) {
    try {
      const outcome = await moduleProposalApply(moduleId, item.id);
      if (outcome.status === "applied") applied += 1;
    } catch {
      // 单条失败不挡其余
    }
  }
  emit("done", t("modules.proposal_applied_all", { applied, total: list.length }));
  await load();
}

async function dismiss(moduleId: string, proposal: ProposalView) {
  try {
    await moduleProposalDismiss(moduleId, proposal.id);
  } catch {
    // 忽略失败不挡人
  }
  await load();
}

onMounted(() => {
  void load();
  timer = window.setInterval(() => void load(), 5000);
});

onUnmounted(() => {
  if (timer !== undefined) window.clearInterval(timer);
});
</script>

<template>
  <section v-if="bars.length > 0 || capped.length > 0" class="modules__candidates">
    <p v-for="bar in capped" :key="`cap-${bar.moduleId}`" class="modules__candidate">
      <span class="modules__candidate-text">
        {{ t("modules.proposal_capped", { name: nameOf(bar.moduleId), count: bar.count }) }}
      </span>
    </p>
    <div v-for="bar in bars" :key="bar.moduleId">
      <p class="modules__candidate">
        <span class="modules__candidate-text">
          {{ t("modules.proposals_bar", { name: nameOf(bar.moduleId), count: bar.list.length }) }}
        </span>
      </p>
      <p v-if="bar.list.filter((item) => !item.stale).length > 1" class="modules__candidate">
        <span class="modules__candidate-text">
          {{ t("modules.proposal_batch_hint", { count: bar.list.filter((item) => !item.stale).length }) }}
        </span>
        <button
          type="button"
          class="modules__button dialog__button"
          :disabled="busy"
          @click="applyAll(bar.moduleId)"
        >
          {{ t("modules.proposal_apply_all") }}
        </button>
      </p>
      <p v-for="item in bar.list" :key="item.id" class="modules__candidate">
        <span class="modules__candidate-text">
          {{
            item.kind === "text"
              ? t("modules.proposal_item", { title: item.title || `#${item.node_id}`, chars: item.chars })
              : item.kind === "rename"
                ? t("modules.proposal_rename", { title: item.title })
                : item.kind === "collect"
                  ? t("modules.proposal_collect", { title: item.title, count: item.nodes.length })
                  : item.kind === "merge"
                    ? t("modules.proposal_merge", { count: item.nodes.length })
                    : t("modules.proposal_move", { parent: item.parent ?? "—" })
          }}
          <span v-if="item.stale" class="modules__proposal-stale">
            {{ t("modules.proposal_stale_badge") }}
          </span>
        </span>
        <button
          type="button"
          class="modules__button dialog__button"
          :disabled="busy || item.stale"
          @click="apply(bar.moduleId, item)"
        >
          {{ t("modules.proposal_apply") }}
        </button>
        <button
          type="button"
          class="modules__button dialog__button"
          :disabled="busy"
          @click="dismiss(bar.moduleId, item)"
        >
          {{ t("modules.proposal_dismiss") }}
        </button>
      </p>
    </div>
  </section>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./module-dialog.css"></style>
