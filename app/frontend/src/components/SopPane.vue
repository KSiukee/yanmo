<script setup lang="ts">
// 「流程」这一片：**你自己的写作流程**——先干什么后干什么。
//
// 三条铁律写在界面上也照做：① **没有"下一步"这道门**（可以跳步 / 回退 / 并行）；
// ② **不是打卡清单**（勾与不勾只是你自己的记号，这里不统计达成率、不提醒）；
// ③ **跟人走**（"我平时的写法"那份是常态），但允许这一本单独一套——两层之间是
// 「不覆盖 / 覆盖」，不做字段级合并。
//
// 这一块只管"页面长什么样"：读哪一层、改一步、存回去全在 [`useSopPanel`](./sop-panel.ts)；
// 一步里那串检查项（人工的 + 机械的）在 [`SopChecks`](./SopChecks.vue)——它有自己的一小份
// 状态（机械项的选择器），摊在这一份里就同时管三层东西了。
//
// 状态就地建（不挂进会话）：改一下就存一下，**换页签 / 切专注再回来重新读一遍库里那份
// 就是对的**——唯一不记住的是"刚才在改哪一层"，那本来就该默认回"我平时的写法"。
import { computed, onMounted, toRef, watch } from "vue";

import { t } from "../locales/index.ts";
import SopChecks from "./SopChecks.vue";
import { useSopPanel } from "./sop-panel.ts";
import { sourceLabel, stepNamePlaceholder, stepNotePlaceholder } from "./sop.ts";

const props = defineProps<{ workId: number | null; nodeId: number | null }>();
const emit = defineEmits<{ "open-tab": [string] }>();

const sop = useSopPanel({
  workId: toRef(props, "workId"),
  nodeId: toRef(props, "nodeId"),
});

const {
  board,
  busy,
  errorText,
  receipt,
  workScope,
  list,
  summaryText,
  canUseWorkScope,
  mechBusy,
  readingOf,
  recheck,
  syncFacts,
  setScope,
  addStep,
  moveStep,
  removeStep,
  toggleSkip,
  renameStep,
  setNote,
  setMinutes,
  addCheck,
  addMech,
  setCheckText,
  toggleCheck,
  removeCheck,
  setAction,
  reset,
} = sop;

// 一挂上就读一次；**换了书**要回到"我平时的写法"那一层重新读
onMounted(() => void setScope(false));
watch(() => props.workId, () => void setScope(false));
// 换章：机械项"本章"那一档的读数要跟着换（单子没变就不重算）
watch(() => props.nodeId, () => void syncFacts());

/** 有机械项才露"重算"那一行（没有就没什么可算的）。 */
const hasMech = computed(() =>
  list.value.some((step) => step.checks.some((check) => check.mech !== null)),
);

/** 输入框里的字（`@change` = 失焦或回车才写库：一处改动一条留痕）。 */
function text(event: Event): string {
  return (event.target as HTMLInputElement).value;
}
</script>

<template>
  <section class="sop">
    <p class="hint">{{ t("sop.rule") }}</p>
    <p v-if="errorText" class="error">{{ errorText }}</p>
    <p v-if="receipt" class="saved">{{ receipt }}</p>

    <!-- 作用范围：我平时的写法 / 这一本单独一套（两层之间是"不覆盖 / 覆盖"） -->
    <div class="row">
      <span class="hint">{{ t("sop.scope.label") }}</span>
      <button
        type="button"
        class="chip"
        :class="{ 'chip--on': !workScope }"
        :disabled="busy"
        @click="void setScope(false)"
      >
        {{ t("sop.scope.global") }}
      </button>
      <button
        type="button"
        class="chip"
        :class="{ 'chip--on': workScope }"
        :disabled="busy || !canUseWorkScope"
        @click="void setScope(true)"
      >
        {{ t("sop.scope.work") }}
      </button>
    </div>
    <p v-if="board" class="hint">{{ sourceLabel(board.source) }} · {{ summaryText }}</p>

    <!-- 机械项是现算的：正文改过之后点一下"重算"（没机械项就不露这一行） -->
    <p v-if="hasMech" class="row">
      <span class="hint">{{ t("sop.mech.hint") }}</span>
      <button class="link" :disabled="busy || mechBusy" @click="void recheck()">
        {{ mechBusy ? t("sop.mech.busy") : t("sop.mech.recheck") }}
      </button>
    </p>

    <ol v-if="list.length > 0" class="steps">
      <li
        v-for="(step, index) in list"
        :key="step.id"
        class="step"
        :class="{ 'step--skipped': step.skipped }"
      >
        <p class="step__head">
          <span class="step__no">{{ index + 1 }}</span>
          <input
            class="step__name"
            :value="step.name ?? ''"
            :placeholder="stepNamePlaceholder(step)"
            :disabled="busy"
            @change="void renameStep(step.id, text($event))"
          />
          <button class="link" :disabled="busy || index === 0" @click="void moveStep(step.id, -1)">
            ↑
          </button>
          <button
            class="link"
            :disabled="busy || index === list.length - 1"
            @click="void moveStep(step.id, 1)"
          >
            ↓
          </button>
          <button class="link" :disabled="busy" @click="void toggleSkip(step.id)">
            {{ step.skipped ? t("sop.unskip") : t("sop.skip") }}
          </button>
          <button class="link" :disabled="busy" @click="void removeStep(step.id)">
            {{ t("sop.remove") }}
          </button>
        </p>

        <input
          class="step__note"
          :value="step.note ?? ''"
          :placeholder="stepNotePlaceholder(step)"
          :disabled="busy"
          @change="void setNote(step.id, text($event))"
        />

        <p class="row">
          <label class="minutes">
            <span class="hint">{{ t("sop.minutes") }}</span>
            <input
              type="number"
              min="1"
              :value="step.minutes ?? ''"
              :disabled="busy"
              @change="void setMinutes(step.id, text($event))"
            />
          </label>
          <span class="hint">{{ t("sop.minutes.hint") }}</span>
        </p>

        <!-- 检查项：作者自己写的一句话 + 勾没勾；或者一条**机械项**（现算，不用勾） -->
        <SopChecks
          :step="step"
          :busy="busy"
          :reading="readingOf"
          @add-check="void addCheck(step.id)"
          @add-mech="(scope, judge) => void addMech(step.id, scope, judge)"
          @toggle-check="(checkId) => void toggleCheck(step.id, checkId)"
          @set-check-text="(checkId, value) => void setCheckText(step.id, checkId, value)"
          @remove-check="(checkId) => void removeCheck(step.id, checkId)"
        />

        <p class="row">
          <!-- 绑一个动作：这一步打开哪块面板（去干那件事，不是"打卡"） -->
          <span class="hint">{{ t("sop.action.label") }}</span>
          <select
            :value="step.action?.value ?? ''"
            :disabled="busy"
            @change="void setAction(step.id, text($event))"
          >
            <option value="">{{ t("sop.action.none") }}</option>
            <option value="flow">{{ t("flow.title") }}</option>
            <option value="creator">{{ t("creator.tab") }}</option>
            <option value="sop">{{ t("sop.tab") }}</option>
          </select>
          <button
            v-if="step.action"
            class="link"
            :disabled="busy"
            @click="emit('open-tab', step.action.value)"
          >
            {{ t("sop.action.open") }}
          </button>
        </p>
      </li>
    </ol>
    <p v-else-if="board && !busy" class="hint">{{ t("sop.empty") }}</p>

    <p class="row">
      <button class="link" :disabled="busy" @click="void addStep()">{{ t("sop.add") }}</button>
      <button class="link" :disabled="busy" @click="void reset()">
        {{ workScope ? t("sop.reset.work") : t("sop.reset.global") }}
      </button>
    </p>
  </section>
</template>

<!-- 与检查项清单共用的那几件（读数 / 行 / 按钮 / 下拉），见 sop.css 的头注 -->
<style scoped src="./sop.css"></style>

<style scoped>
/* 这一片是右栏的直接子件（与叩问 / 创作流同层）：**内边距与滚动都归它自己**——
   少了这两样，内容就会贴着栏边（2026-09-20 用户一眼看出来的）。 */
.sop {
  flex: 1 1 auto;
  min-height: 0;
  padding: 12px;
  overflow: auto;
  font-size: 13px;
}

.error {
  margin: 0 0 6px;
  font-size: 11px;
  color: var(--ym-danger, #c0392b);
}

.saved {
  margin: 0 0 6px;
  font-size: 11px;
  opacity: 0.8;
}

.chip {
  padding: 2px 8px;
  border: 1px solid var(--ym-line);
  border-radius: 999px;
  background: transparent;
  color: inherit;
  font: inherit;
  font-size: 11px;
  cursor: pointer;
}

.chip--on {
  border-color: var(--ym-accent);
  color: var(--ym-accent);
}

.chip:disabled {
  opacity: 0.5;
  cursor: default;
}

.steps {
  margin: 0 0 8px;
  padding: 0;
  list-style: none;
}

.step {
  margin: 0 0 10px;
  padding: 0 0 8px;
  border-bottom: 1px solid var(--ym-line);
}

.step--skipped {
  opacity: 0.55;
}

.step__head {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  align-items: center;
  margin: 0 0 4px;
}

.step__no {
  min-width: 16px;
  font-size: 11px;
  font-weight: 600;
  opacity: 0.7;
}

.step__name,
.step__note {
  box-sizing: border-box;
  padding: 2px 6px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: var(--ym-paper);
  color: inherit;
  font: inherit;
}

.step__name {
  flex: 1 1 120px;
  font-size: 12px;
  font-weight: 600;
}

.step__note {
  width: 100%;
  margin: 0 0 4px;
  font-size: 11px;
}

.minutes {
  display: inline-flex;
  gap: 4px;
  align-items: center;
  font-size: 11px;
}

.minutes input {
  width: 64px;
  padding: 2px 6px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: var(--ym-paper);
  color: inherit;
  font: inherit;
  font-size: 11px;
}
</style>
