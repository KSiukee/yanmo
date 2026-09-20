<script setup lang="ts">
// 记一次投稿 / 改一条记录那张表单：**同一个表单**，只是标题与落点不同。
//
// 它自己管"填到一半的那几个字段"（换一篇就重填），落点由外面给——
// 这里只管"填好了、点了记下"，以及把判词那句原样摆出来。
//
// 软拦的交互就在这一屏：判词不是 clear 时，正文换成"仍然记下 + 返回改一改"，
// 输入框先锁住（作者正在读后果，不该一边读一边把目标改了）。
import { ref, watch } from "vue";

import type { SubmitDraft, ManuscriptForm, WarnedSubmit } from "../editor/manuscript-board";
import { dateInputToMs, defaultCooldownUntil, msToDateInput, verdictKey } from "../editor/manuscript";
import { t } from "../locales/index.ts";

const props = defineProps<{
  form: ManuscriptForm;
  defaultCooldownDays: number;
  busy: boolean;
  warned: WarnedSubmit | null;
}>();

const emit = defineEmits<{
  save: [SubmitDraft];
  confirm: [];
  dismiss: [];
  cancel: [];
}>();

const draft = ref({ venue: "", date: "", cooldown: "", notes: "" });
const formError = ref("");

/** 打开 / 换一条：把字段按这一条（或"新的一次"）重填。 */
function fillForm(): void {
  const record = props.form.submission;
  const now = Date.now();
  draft.value = record
    ? {
        venue: record.venue,
        date: msToDateInput(record.submitted_at),
        cooldown: msToDateInput(record.cooldown_until),
        notes: record.notes,
      }
    : {
        venue: "",
        date: msToDateInput(now),
        cooldown: msToDateInput(defaultCooldownUntil(now, props.defaultCooldownDays)),
        notes: "",
      };
  formError.value = "";
}

watch(() => props.form, fillForm, { immediate: true });

/** 判词那句：把"还在等谁 / 到哪天 / 这家投过"的参数一起摆出来。 */
function warnText(): string {
  const pending = props.warned;
  if (!pending) return "";
  return t(verdictKey(pending.verdict.code), {
    venue: pending.verdict.venue ?? "",
    at: msToDateInput(pending.verdict.at),
    until: msToDateInput(pending.verdict.cooldown_until) || t("manuscript.no_deadline"),
  });
}

function submit() {
  const venue = draft.value.venue.trim();
  if (!venue) {
    formError.value = t("manuscript.venue_required");
    return;
  }
  formError.value = "";
  emit("save", {
    venue,
    submittedAt: dateInputToMs(draft.value.date) ?? Date.now(),
    cooldownUntil: draft.value.cooldown.trim() === "" ? null : dateInputToMs(draft.value.cooldown),
    notes: draft.value.notes.trim(),
  });
}
</script>

<template>
  <div class="manuscript__form">
    <h3 class="manuscript__form-title">
      {{ form.submission ? t("manuscript.edit_record") : t("manuscript.new_record") }}
    </h3>
    <label class="manuscript__field">
      <span>{{ t("manuscript.field_venue") }}</span>
      <input v-model="draft.venue" class="manuscript__input" :disabled="warned !== null" />
    </label>
    <label class="manuscript__field">
      <span>{{ t("manuscript.field_date") }}</span>
      <input v-model="draft.date" type="date" class="manuscript__input" :disabled="warned !== null" />
    </label>
    <label class="manuscript__field">
      <span>{{ t("manuscript.field_cooldown") }}</span>
      <input v-model="draft.cooldown" type="date" class="manuscript__input" :disabled="warned !== null" />
    </label>
    <label class="manuscript__field">
      <span>{{ t("manuscript.field_notes") }}</span>
      <input v-model="draft.notes" class="manuscript__input" :disabled="warned !== null" />
    </label>
    <p v-if="formError" class="manuscript__warn">{{ formError }}</p>
    <p v-if="warned" class="manuscript__warn">{{ warnText() }}</p>
    <div class="manuscript__form-actions">
      <button
        v-if="warned === null"
        type="button"
        class="manuscript__button dialog__button"
        :disabled="busy"
        @click="submit"
      >
        {{ t("manuscript.save_record") }}
      </button>
      <button
        v-else
        type="button"
        class="manuscript__button manuscript__button--danger dialog__button"
        :disabled="busy"
        @click="emit('confirm')"
      >
        {{ t("manuscript.submit_anyway") }}
      </button>
      <button
        type="button"
        class="manuscript__button dialog__button"
        :disabled="busy"
        @click="warned !== null ? emit('dismiss') : emit('cancel')"
      >
        {{ warned !== null ? t("manuscript.back") : t("common.cancel") }}
      </button>
    </div>
  </div>
</template>

<style scoped src="./manuscript-dialog.css"></style>
