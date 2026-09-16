<script setup lang="ts">
// 「大纲」面板里**人物 / 设定**那一页（同一个组件，两档字段不一样）。
//
// 为什么两档合成一个组件：它们存的是同一张表、同一套读写（名字 + 键值 + 一段自由文字），
// 差别只有"摆哪几行、叫什么名"——写两遍就是两份迟早长歪的副本。
//
// 字段口径（用户 2026-09-16 真机定的）：
// - **人物**：名字（必填）+ 别称 + 设定（键值）+ 备注；
// - **设定**（地点 / 物品 / 一条规矩）：名称（必填）+ 条目（键值）+ 内容说明。
//   **不摆别称**——一个人可以有字、号、绰号，一个地方不需要。
import type { EntityCard, EntityKind } from "../api/entity.ts";
import { t } from "../locales/index.ts";
import { cardHeadline, visibleCards } from "./entity.ts";
import type { EditorSession } from "../editor/session.ts";

const props = defineProps<{
  session: EditorSession;
  /** 这一页是哪一档（页签决定） */
  kind: EntityKind;
}>();

const {
  board,
  busy,
  errorText,
  draft,
  justSaved,
  canSave,
  startNew,
  startEdit,
  cancelEdit,
  save,
  remove,
} = props.session.entities;
const { workId } = props.session;

/** 这一页要摆的那几张（页签本身就是筛子，不必再摆一排筛选项）。 */
const cards = () => visibleCards(board.value?.cards ?? [], props.kind);

/** 删之前问一句（与目录树删章同一个习惯）。 */
function askRemove(card: EntityCard) {
  if (window.confirm(t("entity.delete_confirm", { name: card.name }))) void remove(card);
}
</script>

<template>
  <section class="entities">
    <p class="entities__rule">
      {{ kind === "person" ? t("entity.rule_person") : t("entity.rule_setting") }}
    </p>
    <p v-if="errorText" class="entities__error">{{ errorText }}</p>
    <p v-if="justSaved" class="entities__saved">{{ justSaved }}</p>
    <p v-if="workId === null" class="entities__hint">{{ t("entity.no_work") }}</p>

    <div class="entities__bar">
      <button
        type="button"
        class="dialog__button"
        :disabled="busy || workId === null"
        @click="startNew(kind)"
      >
        {{ t("entity.new") }}
      </button>
      <span class="entities__meta">{{ t("entity.count", { count: cards().length }) }}</span>
    </div>

    <!-- 表单：新建或改一张（整卡覆盖） -->
    <form v-if="draft" class="entities__form" @submit.prevent="void save()">
      <label class="entities__field">
        <span class="entities__label">
          {{ kind === "person" ? t("entity.name") : t("entity.setting_name") }}
        </span>
        <input
          v-model="draft.name"
          class="entities__input"
          :placeholder="t('entity.name_placeholder')"
          :disabled="busy"
        />
      </label>
      <label v-if="kind === 'person'" class="entities__field">
        <span class="entities__label">{{ t("entity.aliases") }}</span>
        <input
          v-model="draft.aliasesText"
          class="entities__input"
          :placeholder="t('entity.aliases_placeholder')"
          :disabled="busy"
        />
      </label>
      <label class="entities__field">
        <span class="entities__label">
          {{ kind === "person" ? t("entity.attributes") : t("entity.setting_entries") }}
        </span>
        <textarea
          v-model="draft.attributesText"
          class="entities__input entities__input--area"
          rows="3"
          :placeholder="t('entity.attributes_placeholder')"
          :disabled="busy"
        ></textarea>
      </label>
      <label class="entities__field">
        <span class="entities__label">
          {{ kind === "person" ? t("entity.note") : t("entity.setting_note") }}
        </span>
        <textarea
          v-model="draft.note"
          class="entities__input entities__input--area"
          rows="3"
          :disabled="busy"
        ></textarea>
      </label>
      <p class="entities__hint">{{ t("entity.form_hint") }}</p>
      <div class="entities__actions">
        <button type="submit" class="dialog__button" :disabled="busy || !canSave">
          {{ t("entity.save") }}
        </button>
        <button type="button" class="dialog__button" :disabled="busy" @click="cancelEdit()">
          {{ t("entity.cancel") }}
        </button>
      </div>
    </form>

    <p v-if="busy && !board" class="entities__hint">{{ t("entity.loading") }}</p>
    <ul v-else-if="cards().length > 0" class="entities__list">
      <li v-for="card in cards()" :key="card.id" class="entities__card">
        <div class="entities__main">
          <span class="entities__name">{{ cardHeadline(card) }}</span>
          <span v-if="card.attributes.length > 0" class="entities__meta">
            {{ t("entity.count", { count: card.attributes.length }) }}
          </span>
          <span v-if="card.note" class="entities__note" :title="card.note">{{ card.note }}</span>
        </div>
        <div class="entities__row-actions">
          <button type="button" class="dialog__button" :disabled="busy" @click="startEdit(card)">
            {{ t("entity.edit") }}
          </button>
          <button type="button" class="dialog__button" :disabled="busy" @click="askRemove(card)">
            {{ t("entity.delete") }}
          </button>
        </div>
      </li>
    </ul>
    <p v-else-if="board && !busy" class="entities__hint">
      {{ kind === "person" ? t("entity.empty") : t("entity.empty_setting") }}
    </p>

  </section>
</template>

<style scoped src="./entity-dialog.css"></style>
