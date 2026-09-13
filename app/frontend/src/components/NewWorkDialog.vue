<script setup lang="ts">
// 建书页：**一次问清**——书名、作品类型、简介、卷/章命名规则，建好直接开写。
//
// 分寸：
// - 作品类型决定后面长什么样（长篇是卷→章；单篇零层级；短篇集是一篇一篇），
//   所以**单篇 / 短篇集不显示"卷/章命名"**（它们本来就不编号），只给一句说明；
// - 命名规则这一栏默认「跟随设置」（不写这本书的覆盖）——想这本单独一套才选别的；
// - 简介与服务端无关，纯作者的话，建完就落库（投稿包的大纲要用它）。
import { computed, nextTick, ref, watch } from "vue";

import { t } from "../locales/index.ts";
import type { EditorSession } from "../editor/session";

const props = defineProps<{ session: EditorSession }>();
const emit = defineEmits<{ close: [] }>();
const { create, busy } = props.session.shelf;
const { values: appearanceValues } = props.session.appearance;

const title = ref("");
const kind = ref("novel");
const summary = ref("");
/** `""` = 跟随设置（不写这本书的覆盖）；否则是 NamingStyle 的稳定代码 */
const naming = ref("");
const titleEl = ref<HTMLInputElement | null>(null);

/** 长篇才谈得上"卷 / 章怎么叫" */
const numbered = computed(() => kind.value === "novel");

/** 「跟随设置」旁边标一下当前设置到底是哪一档（读不到就只说"跟随设置"） */
const followLabel = computed(() => {
  const code = appearanceValues.value?.naming ?? (numbered.value ? "arabic" : "none");
  return t("shelf.naming_follow", { name: t(`settings.naming.${code}`) });
});

const OPTIONS = ["arabic", "chinese", "padded", "none"] as const;

watch(kind, () => {
  // 换成单篇 / 短篇集就不再显示命名那一栏：把选择清回"跟随设置"，免得多带一个覆盖进库
  if (!numbered.value) naming.value = "";
});

void nextTick(() => titleEl.value?.focus());

function submit() {
  const name = title.value.trim();
  if (!name || busy.value) return;
  void create({
    kind: kind.value,
    title: name,
    summary: summary.value,
    naming: numbered.value && naming.value !== "" ? naming.value : null,
  });
  emit("close");
}
</script>

<template>
  <div class="newwork dialog dialog--above" @click.self="emit('close')">
    <section class="newwork__box dialog__box">
      <header class="newwork__head dialog__head">
        <h2 class="newwork__title dialog__title">{{ t("shelf.new_work_title") }}</h2>
        <button type="button" class="newwork__button dialog__button" @click="emit('close')">
          {{ t("common.cancel") }}
        </button>
      </header>

      <form class="newwork__form" @submit.prevent="submit">
        <label class="newwork__row">
          <span class="newwork__label">{{ t("shelf.field_title") }}</span>
          <input
            ref="titleEl"
            v-model="title"
            class="newwork__input dialog__input"
            type="text"
            :placeholder="t('shelf.name_placeholder')"
            @keydown.esc="emit('close')"
          />
        </label>

        <label class="newwork__row">
          <span class="newwork__label">{{ t("shelf.field_kind") }}</span>
          <select v-model="kind" class="newwork__select">
            <option value="novel">{{ t("shelf.kind_novel") }}</option>
            <option value="collection">{{ t("shelf.kind_collection") }}</option>
            <option value="article">{{ t("shelf.kind_article") }}</option>
          </select>
        </label>

        <label class="newwork__row newwork__row--block">
          <span class="newwork__label">{{ t("shelf.field_summary") }}</span>
          <textarea
            v-model="summary"
            class="newwork__summary"
            rows="3"
            :placeholder="t('shelf.summary_placeholder')"
          />
        </label>

        <!-- 卷 / 章命名：只有长篇显示（单篇与短篇集不编号，名字由作者自己起） -->
        <label v-if="numbered" class="newwork__row">
          <span class="newwork__label">{{ t("shelf.field_naming") }}</span>
          <select v-model="naming" class="newwork__select">
            <option value="">{{ followLabel }}</option>
            <option v-for="code in OPTIONS" :key="code" :value="code">
              {{ t(`settings.naming.${code}`) }}
            </option>
          </select>
        </label>
        <p v-else class="newwork__hint">{{ t("shelf.naming_not_needed") }}</p>

        <p class="newwork__hint">{{ t("shelf.new_work_hint") }}</p>

        <button type="submit" class="newwork__button dialog__button" :disabled="busy || !title.trim()">
          {{ t("shelf.create_and_write") }}
        </button>
      </form>
    </section>
  </div>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./newwork-dialog.css"></style>
