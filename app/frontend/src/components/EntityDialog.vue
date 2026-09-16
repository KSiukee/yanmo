<script setup lang="ts">
// 设定卡（人物 / 设定）：**大纲冲突检测的数据源**——记下来，体检才判得动。
//
// 这个文件只管**长什么样**：会发生什么（列、建、改、删）全在
// [`useEntityPanel`](./entity-panel.ts) 里；两个纯函数（拆别称、读设定）在 `entity.ts`。
//
// 会话由布局层建一次（这里只把它摊开）——与书架、回收站那些弹窗同一条口径。
import type { EntityCard } from "../api/entity.ts";
import type { EditorSession } from "../editor/session.ts";
import { t } from "../locales/index.ts";
import { cardHeadline, kindLabel } from "./entity.ts";

const props = defineProps<{ session: EditorSession }>();
const {
  board,
  busy,
  errorText,
  filter,
  draft,
  justSaved,
  options,
  list,
  canSave,
  hide,
  startNew,
  startEdit,
  cancelEdit,
  save,
  remove,
} = props.session.entities;
const { workId } = props.session;

/** 删之前问一句（与目录树删章同一个习惯）。 */
function askRemove(card: EntityCard) {
  if (window.confirm(t("entity.delete_confirm", { name: card.name }))) void remove(card);
}
</script>

<template>
  <div class="entities dialog" @click.self="hide()">
    <section class="entities__box dialog__box">
      <header class="entities__head dialog__head">
        <h2 class="entities__title dialog__title">{{ t("entity.title") }}</h2>
        <button type="button" class="dialog__button" :disabled="busy || workId === null" @click="startNew()">
          {{ t("entity.new") }}
        </button>
        <button type="button" class="dialog__button" @click="hide()">{{ t("entity.close") }}</button>
      </header>

      <p class="entities__rule">{{ t("entity.rule") }}</p>
      <p v-if="errorText" class="entities__error">{{ errorText }}</p>
      <p v-if="justSaved" class="entities__saved">{{ justSaved }}</p>
      <p v-if="workId === null" class="entities__hint">{{ t("entity.no_work") }}</p>

      <!-- 表单：新建或改一张（整卡覆盖） -->
      <form v-if="draft" class="entities__form" @submit.prevent="void save()">
        <label class="entities__field">
          <span class="entities__label">{{ t("entity.kind") }}</span>
          <select v-model="draft.kind" :disabled="busy">
            <option value="person">{{ kindLabel("person") }}</option>
            <option value="setting">{{ kindLabel("setting") }}</option>
          </select>
        </label>
        <label class="entities__field">
          <span class="entities__label">{{ t("entity.name") }}</span>
          <input
            v-model="draft.name"
            class="entities__input"
            :placeholder="t('entity.name_placeholder')"
            :disabled="busy"
          />
        </label>
        <label class="entities__field">
          <span class="entities__label">{{ t("entity.aliases") }}</span>
          <input
            v-model="draft.aliasesText"
            class="entities__input"
            :placeholder="t('entity.aliases_placeholder')"
            :disabled="busy"
          />
        </label>
        <label class="entities__field">
          <span class="entities__label">{{ t("entity.attributes") }}</span>
          <textarea
            v-model="draft.attributesText"
            class="entities__input entities__input--area"
            rows="3"
            :placeholder="t('entity.attributes_placeholder')"
            :disabled="busy"
          ></textarea>
        </label>
        <label class="entities__field">
          <span class="entities__label">{{ t("entity.note") }}</span>
          <input v-model="draft.note" class="entities__input" :disabled="busy" />
        </label>
        <div class="entities__actions">
          <button type="submit" class="dialog__button" :disabled="busy || !canSave">
            {{ t("entity.save") }}
          </button>
          <button type="button" class="dialog__button" :disabled="busy" @click="cancelEdit()">
            {{ t("entity.cancel") }}
          </button>
        </div>
      </form>

      <!-- 筛选项：数字来自核心（不是数当前这一屏） -->
      <nav v-if="board" class="entities__filters">
        <button
          v-for="option in options"
          :key="option.kind"
          type="button"
          class="entities__chip"
          :class="{ 'entities__chip--on': filter === option.kind }"
          :disabled="busy"
          @click="filter = option.kind"
        >
          {{ option.kind === "all" ? t("entity.filter.all") : kindLabel(option.kind) }}
          <span class="entities__n">{{ t("entity.count", { count: option.count }) }}</span>
        </button>
      </nav>

      <p v-if="busy && !board" class="entities__hint">{{ t("entity.loading") }}</p>
      <ul v-else-if="list.length > 0" class="entities__list">
        <li v-for="card in list" :key="card.id" class="entities__card">
          <div class="entities__main">
            <span class="entities__kind">{{ kindLabel(card.kind) }}</span>
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
            <button
              type="button"
              class="dialog__button"
              :disabled="busy"
              @click="askRemove(card)"
            >
              {{ t("entity.delete") }}
            </button>
          </div>
        </li>
      </ul>
      <p v-else-if="board && !busy" class="entities__hint">{{ t("entity.empty") }}</p>
    </section>
  </div>
</template>

<style scoped src="./entity-dialog.css"></style>
