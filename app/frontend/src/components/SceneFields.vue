<script setup lang="ts">
// 场景卡的四格：**只有场景卡才出现**的一小张表单（视角 / 目标 / 冲突 / 结果）。
//
// 与"章纲一句话"并排（都在正文上方）：同一种东西——低频手填、单独存、不进正文。
// 这个文件只管长什么样；状态机在 [`useSceneFields`](../editor/scene.ts)。
import { t } from "../locales/index.ts";
import type { EditorSession } from "../editor/session.ts";

const props = defineProps<{ session: EditorSession }>();
const { isScene, draft, busy, saved, save } = props.session.scene;

/** 四格的顺序（与核心 `SceneField::ALL` 一致）。 */
const FIELDS = ["pov", "goal", "conflict", "outcome"] as const;
</script>

<template>
  <form v-if="isScene" class="scene" @submit.prevent="void save()">
    <p class="scene__head">
      <span class="scene__title">{{ t("editor.scene.title") }}</span>
      <span class="scene__hint">{{ t("editor.scene.hint") }}</span>
    </p>
    <div class="scene__grid">
      <label v-for="field in FIELDS" :key="field" class="scene__field">
        <span class="scene__label">{{ t(`editor.scene.${field}`) }}</span>
        <input
          v-model="draft[field]"
          class="scene__input"
          type="text"
          :disabled="busy"
          @keydown.esc="($event.target as HTMLInputElement).blur()"
        />
      </label>
    </div>
    <div class="scene__actions">
      <button type="submit" class="scene__button" :disabled="busy">
        {{ t("editor.scene.save") }}
      </button>
      <span v-if="saved" class="scene__saved">{{ t("editor.scene.saved") }}</span>
    </div>
  </form>
</template>

<style scoped>
.scene {
  margin: 0;
  padding: 6px 12px 8px;
  border-bottom: 1px solid var(--ym-line);
  background: var(--ym-paper-dim);
}

.scene__head {
  display: flex;
  gap: 8px;
  align-items: baseline;
  margin: 0 0 4px;
  font-size: 11px;
}

.scene__title {
  letter-spacing: 0.08em;
  opacity: 0.85;
}

.scene__hint {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  opacity: 0.6;
}

.scene__grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 4px 10px;
}

.scene__field {
  display: flex;
  gap: 6px;
  align-items: center;
  min-width: 0;
}

.scene__label {
  flex: none;
  width: 5.5em;
  font-size: 11px;
  opacity: 0.7;
}

.scene__input {
  box-sizing: border-box;
  flex: 1;
  min-width: 0;
  padding: 2px 6px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: var(--ym-paper);
  color: inherit;
  font: inherit;
  font-size: 12px;
}

.scene__actions {
  display: flex;
  gap: 8px;
  align-items: center;
  margin-top: 6px;
}

.scene__button {
  padding: 2px 10px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: var(--ym-paper);
  color: inherit;
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}

.scene__button:disabled {
  opacity: 0.5;
  cursor: default;
}

.scene__saved {
  font-size: 11px;
  opacity: 0.7;
}
</style>
