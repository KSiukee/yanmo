<script setup lang="ts">
// 设置里的「叩问」那一栏：**语气**（怎么问）+ **打扰度**（什么时候别问）。
//
// 单独成件的原因与排版那一栏同一条：这一栏会随语气档与护栏长，而设置面板本体
// （分区导航、其它几栏）的变化理由跟它不一样。它只读设置、只写设置，不碰别的东西。
import { computed } from "vue";

import { t } from "../locales/index.ts";
import type { EditorSession } from "../editor/session";

const props = defineProps<{ session: EditorSession }>();
const { values, busy, setQuestionTone, setQuestionPush } = props.session.appearance;

const unavailable = computed(() => values.value === null);

/** 三档语气：`neutral` 就是"不加语气"＝把这层关掉（写进界面上，省得作者找不到开关）。 */
const TONE_OPTIONS = [
  { code: "warm", key: "settings.tone.warm" },
  { code: "neutral", key: "settings.tone.neutral" },
  { code: "direct", key: "settings.tone.direct" },
] as const;

const tone = computed(() => values.value?.question_tone ?? "warm");
/** 每天最多几次（0 = 不打扰）；只限"推"，面板不受它管。 */
const pushPerDay = computed(() => values.value?.question_push_per_day ?? 3);
const pushCooldown = computed(() => values.value?.question_push_cooldown_minutes ?? 60);

/** 两个数各改一个：另一个按当前值带上（核心那条路是"只写传进来的项"）。 */
function setPerDay(event: Event) {
  const days = Number((event.target as HTMLInputElement).value);
  void setQuestionPush(days, pushCooldown.value);
}
function setCooldown(event: Event) {
  const minutes = Number((event.target as HTMLInputElement).value);
  void setQuestionPush(pushPerDay.value, minutes);
}
</script>

<template>
  <p class="settings__group">{{ t("settings.group_asking") }}</p>

  <div class="settings__row settings__row--stack">
    <span>{{ t("settings.tone") }}</span>
    <span class="settings__choices">
      <label v-for="option in TONE_OPTIONS" :key="option.code">
        <input
          type="radio"
          name="question-tone"
          :checked="tone === option.code"
          :disabled="busy || unavailable"
          @change="void setQuestionTone(option.code)"
        />
        {{ t(option.key) }}
      </label>
    </span>
  </div>
  <p class="settings__hint">{{ t("settings.tone_hint") }}</p>

  <label class="settings__row">
    <span>{{ t("settings.push_per_day") }}</span>
    <input
      class="settings__number"
      type="number"
      min="0"
      max="20"
      :value="pushPerDay"
      :disabled="busy || unavailable"
      @change="setPerDay"
    />
  </label>
  <label class="settings__row">
    <span>{{ t("settings.push_cooldown") }}</span>
    <input
      class="settings__number"
      type="number"
      min="0"
      max="1440"
      :value="pushCooldown"
      :disabled="busy || unavailable"
      @change="setCooldown"
    />
  </label>
  <p class="settings__hint">{{ t("settings.push_hint") }}</p>
</template>
