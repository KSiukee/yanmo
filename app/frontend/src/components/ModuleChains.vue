<script setup lang="ts">
// 链路面板：谁叫过谁、各几次、断没断。断/接都在这里，**断电即断，重启还在**。
import { onMounted, ref } from "vue";

import {
  moduleEdgeCut,
  moduleEdgeRestore,
  moduleEdges,
  type ChainEdgeView,
} from "../api/modules";
import { t } from "../locales/index.ts";

const props = defineProps<{ busy: boolean }>();
const emit = defineEmits<{ done: [message: string] }>();

const edges = ref<ChainEdgeView[]>([]);
const open = ref(false);

async function load() {
  try {
    edges.value = await moduleEdges();
  } catch {
    edges.value = [];
  }
}

function when(at: number): string {
  const days = Math.floor((Date.now() - at) / 86_400_000);
  return days <= 0 ? t("modules.edge_today") : t("modules.edge_days", { days });
}

async function cut(edge: ChainEdgeView) {
  try {
    await moduleEdgeCut(edge.from, edge.to);
    emit(
      "done",
      t("modules.edge_cut_done", { from: edge.from_name, to: edge.to_name }),
    );
    await load();
  } catch (error) {
    emit("done", t("modules.action_failed", { detail: String(error) }));
  }
}

async function restore(edge: ChainEdgeView) {
  try {
    await moduleEdgeRestore(edge.from, edge.to);
    emit(
      "done",
      t("modules.edge_restore_done", { from: edge.from_name, to: edge.to_name }),
    );
    await load();
  } catch (error) {
    emit("done", t("modules.action_failed", { detail: String(error) }));
  }
}

onMounted(() => void load());
</script>

<template>
  <section class="modules__candidates">
    <p class="modules__candidate">
      <span class="modules__candidate-text">
        {{ t("modules.edges_title", { count: edges.length }) }}
      </span>
      <button type="button" class="modules__button dialog__button" @click="open = !open">
        {{ open ? t("modules.words_close") : t("modules.words_edit") }}
      </button>
    </p>
    <template v-if="open">
      <p v-if="edges.length === 0" class="modules__candidate modules__note dialog__note">
        {{ t("modules.edge_none") }}
      </p>
      <p v-for="edge in edges" :key="`${edge.from}->${edge.to}`" class="modules__candidate">
        <span class="modules__candidate-text">
          {{
            t("modules.edge_item", {
              from: edge.from_name,
              to: edge.to_name,
              runs: edge.runs,
              when: when(edge.last_at),
            })
          }}
          <span v-if="edge.cut" class="modules__proposal-stale">
            {{ t("modules.edge_cut_badge") }}
          </span>
        </span>
        <button
          v-if="!edge.cut"
          type="button"
          class="modules__button dialog__button dialog__button--danger"
          :disabled="busy"
          @click="cut(edge)"
        >
          {{ t("modules.edge_cut") }}
        </button>
        <button
          v-else
          type="button"
          class="modules__button dialog__button"
          :disabled="busy"
          @click="restore(edge)"
        >
          {{ t("modules.edge_restore") }}
        </button>
      </p>
    </template>
  </section>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./module-dialog.css"></style>
