<script setup lang="ts">
// 模块事实页：**四组事实 + 一句话总结**，不做评分、不替作者判断。
//
// 三条呈现纪律：① 一句话总结优先（下面那行大字），数字折叠在四组里；
// ② 只给事实——这里没有任何"安全评分"或颜色分级；③ 标明谁说的——每组标题后面写着
// 观测来源（核心 / 壳 / 模块自报）。
import { computed, onMounted, ref } from "vue";

import { moduleFacts, type ModuleFactsView } from "../api/modules";
import { readsLabel } from "./moduleLabels";
import { t } from "../locales/index.ts";

const props = defineProps<{ moduleId: string; moduleName: string }>();
const emit = defineEmits<{ close: [] }>();

const view = ref<ModuleFactsView | null>(null);
const error = ref("");

function sizeText(bytes: number): string {
  return bytes >= 1024 * 1024
    ? `${(bytes / (1024 * 1024)).toFixed(1)} MB`
    : `${Math.ceil(bytes / 1024)} KB`;
}

/** 毫秒时间戳 → 「今天 / N 天前 / 从没」。 */
function whenText(at: number): string {
  if (!at) return t("facts.never");
  const days = Math.floor((Date.now() - at) / 86_400_000);
  return days <= 0 ? t("facts.today") : t("display.days_ago", { days });
}

onMounted(async () => {
  try {
    view.value = await moduleFacts(props.moduleId);
  } catch (failure) {
    error.value = failure instanceof Error ? failure.message : String(failure);
  }
});

/** 一句话总结：状态 · 数据 · 跑过几次 · 出网。 */
const summary = computed(() => {
  const current = view.value;
  if (!current) return "";
  const { facts, network } = current;
  const state = facts.isolated
    ? t("facts.state.isolated")
    : facts.degraded
      ? t("facts.state.degraded")
      : t("facts.state.normal");
  const data = t("facts.summary.data", { size: sizeText(facts.data_bytes) });
  const runs = facts.runs === 0 ? t("facts.never_run") : t("facts.run_times", { count: facts.runs });
  const net =
    network.requests === 0
      ? t("facts.summary.offline")
      : t("facts.summary.online", { host: network.last_host });
  return [state, data, runs, net].join(" · ");
});

/** 读过的书 → 一句「《长夜》12 次、《旧稿》3 次」。 */
const booksLine = computed(() => {
  const books = view.value?.reads.books ?? [];
  return books
    .map((book) =>
      t("facts.book_item", {
        title: book.title ?? t("facts.book_unknown", { id: book.work_id }),
        reads: book.reads,
      }),
    )
    .join(t("common.list_separator"));
});

/** 声明的能力（模块自报的那一份）。 */
const declared = computed(() => {
  const current = view.value;
  if (!current) return "";
  const parts = [readsLabel(current.card)];
  if (current.card.own_data) parts.push(t("modules.own_data"));
  parts.push(current.card.network_declared ? t("modules.network") : t("facts.network_not_declared"));
  return parts.filter(Boolean).join(t("common.list_separator"));
});
</script>

<template>
  <section class="facts">
    <header class="facts__head">
      <h3 class="facts__title">{{ t("facts.title", { name: moduleName }) }}</h3>
      <button type="button" class="facts__button dialog__button" @click="emit('close')">
        {{ t("common.close") }}
      </button>
    </header>

    <p v-if="error" class="facts__error">{{ t("facts.load_failed", { detail: error }) }}</p>

    <template v-if="view">
      <p class="facts__summary">{{ summary }}</p>

      <h4 class="facts__group">{{ t("facts.group.space") }}<span class="facts__who">{{ t("facts.who.core_shell") }}</span></h4>
      <ul class="facts__rows">
        <li>{{ t("facts.program", { size: sizeText(view.program_bytes) }) }}</li>
        <li>
          {{ t("facts.data", { size: sizeText(view.facts.data_bytes) }) }}
          <span v-if="view.facts.data_bytes_partial" class="facts__note">{{ t("facts.data_partial") }}</span>
        </li>
        <li>{{ t("facts.tables", { tables: view.facts.tables, rows: view.facts.rows }) }}</li>
        <li>{{ t("facts.db", { size: sizeText(view.facts.db_bytes) }) }}</li>
        <li>{{ t("facts.mirror", { when: whenText(view.facts.mirror_written_at) }) }}</li>
      </ul>

      <h4 class="facts__group">{{ t("facts.group.behaviour") }}<span class="facts__who">{{ t("facts.who.core_shell") }}</span></h4>
      <ul class="facts__rows">
        <li>{{ t("facts.runs", { count: view.facts.runs, when: whenText(view.facts.last_run_at) }) }}</li>
        <li>{{ t("facts.rejected", { count: view.facts.rejected }) }}</li>
        <li>
          {{ t("facts.reads", {
            reads: view.reads.reads,
            chars: view.reads.chars,
            denied: view.reads.denied,
          }) }}
        </li>
        <li>
          {{ t("facts.network_count", {
            requests: view.network.requests,
            ok: view.network.ok,
            failed: view.network.failed,
            size: sizeText(view.network.bytes),
          }) }}
        </li>
        <li v-if="view.network.requests > 0">
          {{ t("facts.network_last", { host: view.network.last_host, when: whenText(view.network.last_at) }) }}
        </li>
        <li class="facts__note">
          {{ t("facts.network_window", { scanned: view.network.scanned }) }}
        </li>
      </ul>

      <h4 class="facts__group">{{ t("facts.group.bounds") }}<span class="facts__who">{{ t("facts.who.declared_core") }}</span></h4>
      <ul class="facts__rows">
        <li>{{ t("facts.declared", { list: declared }) }}</li>
        <li>{{ t("facts.actual_network", { requests: view.network.requests, failed: view.network.failed }) }}</li>
        <li v-if="view.reads.books.length === 0" class="facts__note">{{ t("facts.reads_none") }}</li>
        <li v-if="view.reads.books.length > 0">{{ t("facts.reads_books", { list: booksLine }) }}</li>
        <li class="facts__note">{{ t("facts.cross_read_missing") }}</li>
      </ul>

      <h4 class="facts__group">{{ t("facts.group.identity") }}<span class="facts__who">{{ t("facts.who.registry") }}</span></h4>
      <ul class="facts__rows">
        <li>{{ t("facts.author", { author: view.card.author, version: view.card.version }) }}</li>
        <li>{{ t("facts.origin", { source: view.card.source, when: whenText(view.card.installed_at) }) }}</li>
        <li>
          {{ view.card.certified
            ? t("facts.certified", { edition: view.card.cert_edition, when: whenText(view.card.certified_at) })
            : t("facts.uncertified") }}
        </li>
        <li>{{ t("facts.fingerprint", { fingerprint: view.card.fingerprint.slice(0, 16) }) }}</li>
        <li>
          {{ view.running
            ? t("facts.running", { idle: view.idle_seconds })
            : t("facts.not_running") }}
        </li>
        <li v-if="view.facts.isolated" class="facts__note">
          {{ t("facts.isolated_note", { reason: view.facts.isolation_reason || t("modules.isolation_no_reason") }) }}
        </li>
      </ul>
    </template>
  </section>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./module-facts.css"></style>
