<script setup lang="ts">
// 作者信息那一栏：**填一次，所有投稿都带上**。
//
// 为什么单独成组件：与排版那一栏同理——设置面板已经很长（上帝棘轮盯着 400 行），
// 而这一栏自成一体（一叠字段 + 两个开关），与别的设置项没有耦合。
//
// 两条口径写在界面上，不藏在文档里：
// - 按期刊惯例，作者简介写在**文末**（笔名、真名、电话、通联、简历）；
// - **身份证号与银行账号默认不写进稿件**（它们是稿费结算用的，不该跟着每份稿子跑）。
import { onMounted, ref } from "vue";

import { t } from "../locales/index.ts";
import { authorRead, authorReset, authorWrite, type Author } from "../api/author";

const draft = ref<Author | null>(null);
const note = ref("");
const busy = ref(false);

/** 一栏一行的顺序（长文本那两栏用多行框）。 */
const FIELDS: { key: keyof Author; label: string; long?: boolean }[] = [
  { key: "pen_name", label: "author.pen_name" },
  { key: "real_name", label: "author.real_name" },
  { key: "phone", label: "author.phone" },
  { key: "email", label: "author.email" },
  { key: "address", label: "author.address" },
  { key: "postcode", label: "author.postcode" },
  { key: "bio", label: "author.bio", long: true },
];

/** 敏感两栏单独一组：它们默认不写进稿件，所以视觉上也分开。 */
const SENSITIVE: { key: keyof Author; label: string }[] = [
  { key: "id_number", label: "author.id_number" },
  { key: "bank", label: "author.bank" },
];

function say(error: unknown): void {
  note.value = error instanceof Error ? error.message : String(error);
}

async function load(): Promise<void> {
  busy.value = true;
  try {
    draft.value = await authorRead();
    note.value = "";
  } catch (error) {
    say(error);
  } finally {
    busy.value = false;
  }
}

onMounted(load);

async function save(): Promise<void> {
  const current = draft.value;
  if (!current) return;
  busy.value = true;
  try {
    draft.value = await authorWrite({ ...current });
    note.value = t("author.saved");
  } catch (error) {
    say(error);
  } finally {
    busy.value = false;
  }
}

/** 清空要先问一句：投稿文件里那一段会跟着没了（稿子本身一个字不动）。 */
async function clear(): Promise<void> {
  if (!window.confirm(t("author.clear_confirm"))) return;
  busy.value = true;
  try {
    draft.value = await authorReset();
    note.value = t("author.cleared");
  } catch (error) {
    say(error);
  } finally {
    busy.value = false;
  }
}

/** 模板里取一个字段的文本值（模板里不做类型转换，收在这一处）。 */
function textOf(key: keyof Author): string {
  const value = draft.value?.[key];
  return typeof value === "string" ? value : "";
}

/** 输入框绑一个字段（`v-model` 用 `draft[key]` 在 Vue 里类型太松，这里收一处）。 */
function onText(key: keyof Author, event: Event): void {
  if (!draft.value) return;
  draft.value[key] = (event.target as HTMLInputElement).value as never;
}

function onToggle(key: keyof Author, event: Event): void {
  if (!draft.value) return;
  draft.value[key] = (event.target as HTMLInputElement).checked as never;
}
</script>

<template>
  <div v-if="draft">
    <p class="settings__group">{{ t("settings.group_author") }}</p>
    <p class="settings__hint">{{ t("author.hint") }}</p>

    <label v-for="field in FIELDS" :key="field.key" class="settings__row settings__row--stack">
      <span class="settings__label">{{ t(field.label) }}</span>
      <textarea
        v-if="field.long"
        class="settings__textarea"
        :value="textOf(field.key)"
        :disabled="busy"
        rows="3"
        @input="onText(field.key, $event)"
      ></textarea>
      <input
        v-else
        class="settings__input"
        type="text"
        :value="textOf(field.key)"
        :disabled="busy"
        @input="onText(field.key, $event)"
      />
    </label>

    <p class="settings__group">{{ t("settings.group_author_sensitive") }}</p>
    <p class="settings__hint">{{ t("author.sensitive_hint") }}</p>
    <label v-for="field in SENSITIVE" :key="field.key" class="settings__row settings__row--stack">
      <span class="settings__label">{{ t(field.label) }}</span>
      <input
        class="settings__input"
        type="text"
        :value="textOf(field.key)"
        :disabled="busy"
        @input="onText(field.key, $event)"
      />
    </label>

    <p class="settings__group">{{ t("settings.group_author_attach") }}</p>
    <label class="settings__row">
      <input
        type="checkbox"
        :checked="draft.attach"
        :disabled="busy"
        @change="onToggle('attach', $event)"
      />
      <span>{{ t("author.attach") }}</span>
    </label>
    <label class="settings__row">
      <input
        type="checkbox"
        :checked="draft.with_sensitive"
        :disabled="busy || !draft.attach"
        @change="onToggle('with_sensitive', $event)"
      />
      <span>{{ t("author.with_sensitive") }}</span>
    </label>

    <div class="settings__footer">
      <button type="button" class="settings__button dialog__button" :disabled="busy" @click="save">
        {{ t("common.save") }}
      </button>
      <button type="button" class="settings__button dialog__button" :disabled="busy" @click="clear">
        {{ t("author.clear") }}
      </button>
      <span v-if="note" class="settings__hint">{{ note }}</span>
    </div>
  </div>
</template>

<style scoped src="./settings-dialog.css"></style>
