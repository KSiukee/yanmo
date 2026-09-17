<script setup lang="ts">
// 「稿子放在哪」那一栏里的**磁盘 .md 镜像**：开关 / 状态 / 打开文件夹 / 立即对一遍 / 待定夺清单。
//
// 单独成组件：设置面板本身已经很长（上帝棘轮盯着 400 行），而这一块自成一体
// （自有状态、自有轮询），与别的设置项没有耦合。
//
// 一条纪律写在最前面：**界面拿到的永远是壳报上来的事实**（共享的那份 `MirrorStatus`），
// 不自己推断"应该已经写好了"。所以打开开关之后要隔一会儿把状态**读回来**，
// 而不是就地显示一句"已同步"——那句可能是假的。
import { computed, onUnmounted, ref } from "vue";

import { t } from "../locales/index.ts";
import { openMirrorDir, setMirrorEnabled, syncMirrorNow } from "../api/mirror";
import { clockText, mirrorLine } from "../editor/mirror.ts";
import { openMirrorReview, refreshMirror, useMirrorReview } from "../editor/mirror-review.ts";

const { status } = useMirrorReview();
const busy = ref(false);
const syncing = ref(false);
const actionError = ref<string | null>(null);
/** 等对账结果的定时器（组件关了要清掉，别让它去改一个已经卸载的界面） */
const timers: number[] = [];

const enabled = computed(() => status.value?.enabled ?? false);
const conflicts = computed(() => status.value?.conflicts ?? 0);
const untracked = computed(() => status.value?.untracked ?? 0);
const issues = computed(() => status.value?.issues.length ?? 0);
const failed = computed(() => status.value?.failed_works ?? 0);
const lastError = computed(() => status.value?.last_error ?? "");

const stateText = computed(() => {
  const line = mirrorLine(status.value);
  if (line.kind === "off") {
    return t("settings.mirror_state_off");
  }
  if (line.kind === "never") {
    return t("settings.mirror_state_never");
  }
  return t("settings.mirror_state_on", { files: line.files, time: clockText(line.at) });
});

function describe(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** 对账是异步的：过一会儿把状态**读回来**（读两次，慢一点的第二遍兜底）。 */
function lookAgain(): void {
  timers.push(
    window.setTimeout(() => void refreshMirror(), 1200),
    window.setTimeout(() => {
      syncing.value = false;
      void refreshMirror();
    }, 3500),
  );
}

async function onToggle(event: Event): Promise<void> {
  const wanted = (event.target as HTMLInputElement).checked;
  busy.value = true;
  actionError.value = null;
  try {
    status.value = await setMirrorEnabled(wanted);
    if (wanted) {
      syncing.value = true;
      lookAgain();
    }
  } catch (error) {
    actionError.value = describe(error);
    await refreshMirror(); // 没改成：把勾选框拉回库里的真实状态
  } finally {
    busy.value = false;
  }
}

async function syncNow(): Promise<void> {
  syncing.value = true;
  actionError.value = null;
  try {
    await syncMirrorNow();
    lookAgain();
  } catch (error) {
    syncing.value = false;
    actionError.value = describe(error);
  }
}

async function openFolder(): Promise<void> {
  actionError.value = null;
  try {
    await openMirrorDir();
  } catch (error) {
    actionError.value = describe(error);
  }
}

void refreshMirror();

onUnmounted(() => {
  timers.forEach((timer) => window.clearTimeout(timer));
});
</script>

<template>
  <p class="settings__group">{{ t("settings.mirror_title") }}</p>
  <label class="settings__row">
    <input type="checkbox" :checked="enabled" :disabled="busy" @change="onToggle" />
    <span>{{ t("settings.mirror_on") }}</span>
  </label>
  <p class="settings__hint">{{ t("settings.mirror_hint") }}</p>
  <p class="settings__row settings__row--path">
    <span class="settings__label">{{ t("settings.mirror_where") }}</span>
    <code class="settings__path">{{ status?.root ?? "" }}</code>
  </p>
  <p class="settings__hint">{{ stateText }}</p>
  <button type="button" class="settings__button dialog__button" @click="void openFolder()">
    {{ t("settings.mirror_open") }}
  </button>
  <button
    type="button"
    class="settings__button dialog__button"
    :disabled="syncing || !enabled"
    @click="void syncNow()"
  >
    {{ syncing ? t("settings.mirror_syncing") : t("settings.mirror_sync") }}
  </button>
  <!-- 待定夺的事：只报个数 + 一个入口，逐条处置在对话框里（它才是给这件事做的屏） -->
  <p v-if="conflicts > 0" class="settings__hint settings__hint--bad">
    {{ t("settings.mirror_conflicts", { count: conflicts }) }}
  </p>
  <button
    v-if="issues > 0"
    type="button"
    class="settings__button dialog__button"
    @click="openMirrorReview()"
  >
    {{ t("settings.mirror_review") }}
  </button>
  <p v-if="untracked > 0" class="settings__hint">{{ t("settings.mirror_untracked", { count: untracked }) }}</p>
  <p v-if="failed > 0" class="settings__hint settings__hint--bad">
    {{ t("settings.mirror_failed", { count: failed }) }}
  </p>
  <p v-if="lastError" class="settings__hint settings__hint--bad">
    {{ t("settings.mirror_error", { detail: lastError }) }}
  </p>
  <p v-if="!enabled" class="settings__hint">{{ t("settings.mirror_off_hint") }}</p>
  <p class="settings__hint">{{ t("settings.mirror_not_backup") }}</p>
  <p v-if="actionError" class="settings__hint settings__hint--bad">{{ actionError }}</p>
</template>

<style scoped src="./settings-dialog.css"></style>
