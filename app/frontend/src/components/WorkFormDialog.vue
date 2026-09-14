<script setup lang="ts">
// 作品表单：**新建与编辑用同一个**——一次问清，别让作者在两个长得不一样的小框里
// 改书名、改简介（那两处以前各写各的，空书名还会撞出核心那句"作品标题不能为空"）。
//
// 分寸：
// - **新建**：书名、作品类型、简介、卷/章命名规则。类型决定后面长什么样（长篇是卷→章；
//   单篇零层级；短篇集是一篇一篇），所以**单篇 / 短篇集不显示"卷/章命名"**；
//   命名规则默认「跟随设置」（不写这本书的覆盖）——想这本单独一套才选别的。
// - **编辑**：只给书名与简介。**类型不给改**（它决定已有目录长什么样）——只读显示一行并
//   说明；**命名规则也不在这里改**（那有「整本换写法」的预览确认流程，两套改法迟早打架）。
// - **接手空壳**（首启那条提示的第二个入口）：与新建同一张表，只是名字落在**已经有的那本
//   空壳**上。空壳还没名字、也还没写一个字，所以这会儿**类型可挑**——同类型就地改名，
//   换了类型由书架层按新类型另建一本（见 `shelf.adopt`）。
// - 简介与服务端无关，纯作者的话，落库后投稿包的大纲要用它。
import { computed, nextTick, ref, watch } from "vue";

import { t } from "../locales/index.ts";
import type { EditorSession } from "../editor/session";
import { shelfKindLabel } from "../editor/shelf";
import type { WorkForm } from "../editor/shelf";

const props = defineProps<{ session: EditorSession; form: WorkForm }>();
const emit = defineEmits<{ close: [] }>();

const editing = computed(() => props.form.mode === "edit");
const adopt = computed(() => props.form.mode === "adopt");
const entry = computed(() => (props.form.mode === "create" ? null : props.form.entry));
/** 类型只在**新建**与**接手空壳**时给挑（编辑时它决定着已有目录，只读） */
const pickKind = computed(() => !editing.value);

const { create, edit, adopt: adoptShell, busy } = props.session.shelf;
const { values: appearanceValues } = props.session.appearance;

const title = ref(entry.value?.title ?? "");
const kind = ref(entry.value?.kind ?? "novel");
const summary = ref(entry.value?.summary ?? "");
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
  const draft = {
    kind: kind.value,
    title: name,
    summary: summary.value,
    naming: numbered.value && naming.value !== "" ? naming.value : null,
  };
  if (props.form.mode === "edit") {
    void edit(props.form.entry.id, { title: name, summary: summary.value });
  } else if (props.form.mode === "adopt") {
    void adoptShell(props.form.entry.id, draft);
  } else {
    void create(draft);
  }
  emit("close");
}
</script>

<template>
  <div class="newwork dialog dialog--above" @click.self="emit('close')">
    <section class="newwork__box dialog__box">
      <header class="newwork__head dialog__head">
        <h2 class="newwork__title dialog__title">
          {{ editing ? t("shelf.edit_work_title") : adopt ? t("shelf.adopt_title") : t("shelf.new_work_title") }}
        </h2>
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

        <!-- 新建与接手空壳：类型可选（它决定这本书长什么样）；编辑：只读展示 + 一句为什么不能改 -->
        <label v-if="pickKind" class="newwork__row">
          <span class="newwork__label">{{ t("shelf.field_kind") }}</span>
          <select v-model="kind" class="newwork__select">
            <option value="novel">{{ t("shelf.kind_novel") }}</option>
            <option value="collection">{{ t("shelf.kind_collection") }}</option>
            <option value="article">{{ t("shelf.kind_article") }}</option>
          </select>
        </label>
        <div v-else class="newwork__row">
          <span class="newwork__label">{{ t("shelf.field_kind") }}</span>
          <span class="newwork__readonly">{{ shelfKindLabel(kind) }}</span>
        </div>
        <p v-if="editing" class="newwork__hint">{{ t("shelf.kind_locked") }}</p>
        <p v-if="adopt" class="newwork__hint">{{ t("shelf.adopt_hint") }}</p>

        <label class="newwork__row newwork__row--block">
          <span class="newwork__label">{{ t("shelf.field_summary") }}</span>
          <textarea
            v-model="summary"
            class="newwork__summary"
            rows="3"
            :placeholder="t('shelf.summary_placeholder')"
          />
        </label>

        <!-- 卷 / 章命名：只有**新建长篇**时给（单篇与短篇集不编号；编辑时不碰它，见文件头） -->
        <template v-if="pickKind">
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
        </template>

        <button type="submit" class="newwork__button dialog__button" :disabled="busy || !title.trim()">
          {{ editing ? t("common.save") : adopt ? t("shelf.adopt_and_write") : t("shelf.create_and_write") }}
        </button>
      </form>
    </section>
  </div>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./newwork-dialog.css"></style>
