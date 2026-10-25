<script setup lang="ts">
// 渐进放行：**等作者回答**的读取请求。自己拉、自己轮询、自己作答——
// 管理页只把它摆进来（模块名从卡片里查，查不到就说 id）。
import { onMounted, onUnmounted, ref } from "vue";

import {
  moduleReadAnswer,
  moduleReadAsks,
  type ModuleCard,
  type ReadAskView,
} from "../api/modules";
import { t } from "../locales/index.ts";

const props = defineProps<{ cards: ModuleCard[]; busy: boolean }>();
const emit = defineEmits<{ answered: [message: string] }>();

const asks = ref<ReadAskView[]>([]);
let timer: number | undefined;

function nameOf(moduleId: string): string {
  return props.cards.find((card) => card.id === moduleId)?.name ?? moduleId;
}

/** 把一件"等你回答"的事说成人话（三种：只告知 / 问要不要放宽 / 第三次终极确认）。 */
function askText(ask: ReadAskView): string {
  const name = nameOf(ask.module_id);
  const book = ask.last_work ?? "";
  if (ask.kind === "first") {
    return book
      ? t("modules.read_ask_first", { name, book })
      : t("modules.read_ask_first_nobook", { name });
  }
  if (ask.kind === "ultimate") {
    return book
      ? t("modules.read_ask_ultimate", { name, count: ask.refusals, book })
      : t("modules.read_ask_ultimate_nobook", { name, count: ask.refusals });
  }
  return t("modules.read_ask_second", { name, count: ask.refusals });
}

async function load() {
  try {
    asks.value = await moduleReadAsks();
  } catch {
    // 读不到就先不显示；下一次轮询再试
  }
}

async function answer(ask: ReadAskView, choice: string) {
  try {
    await moduleReadAnswer(ask.module_id, choice);
    emit(
      "answered",
      choice === "allow_more"
        ? t("modules.read_answered_allow", { name: nameOf(ask.module_id) })
        : choice === "keep_limited"
          ? t("modules.read_answered_keep")
          : t("modules.read_answered_ack"),
    );
  } catch (error) {
    emit(
      "answered",
      t("modules.action_failed", {
        detail: error instanceof Error ? error.message : String(error),
      }),
    );
  }
  await load();
}

onMounted(() => {
  void load();
  timer = window.setInterval(() => {
    void load();
  }, 3000);
});

onUnmounted(() => {
  if (timer !== undefined) window.clearInterval(timer);
});
</script>

<template>
  <section v-if="asks.length > 0" class="modules__asks">
    <p v-for="ask in asks" :key="ask.module_id" class="modules__ask">
      <span class="modules__ask-text">{{ askText(ask) }}</span>
      <button
        v-if="ask.kind === 'first'"
        type="button"
        class="modules__button dialog__button"
        :disabled="busy"
        @click="answer(ask, 'acknowledge')"
      >
        {{ t("modules.read_ack") }}
      </button>
      <template v-else>
        <button
          type="button"
          class="modules__button dialog__button"
          :disabled="busy"
          @click="answer(ask, 'allow_more')"
        >
          {{ t("modules.read_allow") }}
        </button>
        <button
          type="button"
          class="modules__button dialog__button"
          :disabled="busy"
          @click="answer(ask, 'keep_limited')"
        >
          {{ t("modules.read_keep") }}
        </button>
      </template>
    </p>
  </section>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./module-dialog.css"></style>
