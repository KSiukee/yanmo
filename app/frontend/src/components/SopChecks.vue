<script setup lang="ts">
// 一步的**检查项清单**：人工那条（作者自己写的一句话 + 勾没勾）与机械那条（现算）。
//
// 单独成件的原因与 `sop.ts` / `sop-panel.ts` 的分工同一条：这一块自己有状态
// （"机械项的选择器开着没有、挑了哪一对判据"），摊在 SopPane 里那一份就同时管三层东西
// （页面骨架 / 一步的字段 / 检查项），那正是不必要的上帝件。
//
// 分寸：读数由外面**算好递进来**（`reading`），这里一个 API 都不调、一个判据都不算——
// 它只管把那一行念出来、把那点交互接出去。
import { ref } from "vue";

import type { SopCheck, SopStep } from "../api/sop.ts";
import { t } from "../locales/index.ts";
import {
  MECH_JUDGES,
  MECH_SCOPES,
  checkPlaceholder,
  mechLabel,
  type MechReading,
} from "./sop.ts";

defineProps<{
  step: SopStep;
  busy: boolean;
  /** 一条检查项现在的读数（机械项才有；人工那条是 null） */
  reading: (check: SopCheck) => MechReading | null;
}>();

const emit = defineEmits<{
  "add-check": [];
  /** 加一条机械项（视角 + 判据，两张单子都在核心的白名单里） */
  "add-mech": [scope: string, judge: string];
  "toggle-check": [checkId: string];
  "set-check-text": [checkId: string, text: string];
  "remove-check": [checkId: string];
}>();

/** 机械项的选择器开着没有（一次只开这一个）。 */
const mechOpen = ref(false);
const mechScope = ref<string>("chapter");
const mechJudge = ref<string>("cast_missing");

function addMech() {
  emit("add-mech", mechScope.value, mechJudge.value);
  mechOpen.value = false;
}

/** 输入框里的字（`@change` = 失焦或回车才写库：一处改动一条留痕）。 */
function text(event: Event): string {
  return (event.target as HTMLInputElement).value;
}

/** 机械项前面的记号：过得去 / 过不去 / **没法判**（还没动笔、没有可判的东西）。 */
function markOf(state: string | undefined): string {
  if (state === "pass") return "✓";
  if (state === "fail") return "!";
  return "–";
}
</script>

<template>
  <ul v-if="step.checks.length > 0" class="checks">
    <li
      v-for="check in step.checks"
      :key="check.id"
      class="check"
      :class="{ 'check--mech': check.mech }"
    >
      <template v-if="check.mech">
        <p class="check__head">
          <span class="check__mark" :class="`check__mark--${reading(check)?.state ?? 'na'}`">{{
            markOf(reading(check)?.state)
          }}</span>
          <span class="check__label">{{ mechLabel(check.mech) }}</span>
          <button class="link" :disabled="busy" @click="emit('remove-check', check.id)">✕</button>
        </p>
        <p class="check__reading" :class="`check__reading--${reading(check)?.state ?? 'na'}`">
          {{ reading(check)?.text }}
        </p>
      </template>
      <template v-else>
        <input
          type="checkbox"
          :checked="check.done"
          :disabled="busy"
          @change="emit('toggle-check', check.id)"
        />
        <input
          class="check__text"
          :value="check.text ?? ''"
          :placeholder="checkPlaceholder(check)"
          :disabled="busy"
          @change="emit('set-check-text', check.id, text($event))"
        />
        <button class="link" :disabled="busy" @click="emit('remove-check', check.id)">✕</button>
      </template>
    </li>
  </ul>

  <p class="row">
    <button class="link" :disabled="busy" @click="emit('add-check')">
      {{ t("sop.checks.add") }}
    </button>
    <button class="link" :disabled="busy" @click="mechOpen = !mechOpen">
      {{ t("sop.mech.add") }}
    </button>
    <template v-if="mechOpen">
      <select v-model="mechScope" :disabled="busy">
        <option v-for="scope in MECH_SCOPES" :key="scope" :value="scope">
          {{ t(`sop.mech.scope.${scope}`) }}
        </option>
      </select>
      <select v-model="mechJudge" :disabled="busy">
        <option v-for="judge in MECH_JUDGES" :key="judge" :value="judge">
          {{ t(`sop.mech.judge.${judge}`) }}
        </option>
      </select>
      <button class="link" :disabled="busy" @click="addMech()">
        {{ t("sop.mech.confirm") }}
      </button>
    </template>
  </p>
</template>

<style scoped src="./sop.css"></style>
