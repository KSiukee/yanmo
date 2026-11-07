<script setup lang="ts">
// 「这本书单独一套」：外观/排版项的每书覆盖开关 + 逐项三态。
//
// 分寸：
// - 开关状态**由库里的稀疏记录决定**（不是本地开关）：刷新/换书后仍然准；
// - 打开 = 把当前生效值写成本书覆盖（这一刻起这本书不再跟着全局走）；
// - 关掉 = 清掉观感这一层（不碰本书的命名规则等其它覆盖）；
// - 下面每一项都能单独「回全局」：只清这一项，别的覆盖保留。
import { computed } from "vue";

import type { EditorSession } from "../editor/session";
import { t } from "../locales/index.ts";
import { WORK_VISUAL_FIELDS, type WorkVisualField } from "../editor/appearance.ts";

const props = defineProps<{ session: EditorSession }>();
const { workMode, workValues, busy, setWorkMode, clearWorkField } = props.session.appearance;

const hasWork = computed(() => props.session.workId.value !== null);

const overrides = computed<WorkVisualField[]>(() => {
  const listed = workValues.value?.overridden ?? [];
  return WORK_VISUAL_FIELDS.filter((field) => listed.includes(field));
});

const FIELD_KEYS: Record<WorkVisualField, string> = {
  editor_font_size: "settings.work_field.editor_font_size",
  editor_line_height: "settings.work_field.editor_line_height",
  editor_letter_spacing: "settings.work_field.editor_letter_spacing",
  editor_font: "settings.work_field.editor_font",
  editor_font_scope: "settings.work_field.editor_font_scope",
  editor_rule_style: "settings.work_field.editor_rule_style",
  editor_bg_color: "settings.work_field.editor_bg_color",
  editor_bg_layout: "settings.work_field.editor_bg_layout",
  editor_bg_fit: "settings.work_field.editor_bg_fit",
  editor_bg_opacity: "settings.work_field.editor_bg_opacity",
};

function onToggle(event: Event) {
  void setWorkMode((event.target as HTMLInputElement).checked);
}
</script>

<template>
  <div class="work-scope">
    <label class="settings__row">
      <input
        type="checkbox"
        :checked="workMode"
        :disabled="busy || !hasWork"
        @change="onToggle"
      />
      <span>{{ t("settings.work_scope") }}</span>
    </label>
    <p class="settings__hint">
      {{ workMode ? t("settings.work_scope_on") : t("settings.work_scope_off") }}
    </p>
    <ul v-if="overrides.length > 0" class="work-scope__list">
      <li v-for="field in overrides" :key="field" class="work-scope__item">
        <span>{{ t(FIELD_KEYS[field]) }}</span>
        <button
          type="button"
          class="settings__button dialog__button"
          :disabled="busy"
          @click="void clearWorkField(field)"
        >
          {{ t("settings.work_scope_clear") }}
        </button>
      </li>
    </ul>
    <p v-else-if="workMode" class="settings__hint">{{ t("settings.work_scope_empty") }}</p>
  </div>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./settings-dialog.css"></style>

<style scoped>
.work-scope {
  margin-bottom: 12px;
}

.work-scope__list {
  list-style: none;
  margin: 6px 0 0;
  padding: 0;
}

.work-scope__item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  font-size: 12px;
  padding: 3px 0;
}
</style>
