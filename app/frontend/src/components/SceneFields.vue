<script setup lang="ts">
// 场景卡的四格：**只有场景卡才出现**的一小张表单（视角 / 目标 / 冲突 / 结果）。
//
// 与"章纲一句话"并排（都在正文上方）：同一种东西——低频手填、单独存、不进正文。
// 这个文件只管长什么样；状态机在 [`useSceneFields`](../editor/scene.ts)。
//
// ★ 收起 / 展开：四格写起来顺手，但**写字的人多数时候只想看着正文**——所以给一个收起按钮
// （用户 2026-09-21 真机提的）。两条分寸：
// - **标题那一行常驻**，按钮在它右端：收起来也不丢入口，"展开"一眼找得回；
// - **不落盘**——这是"当下想看什么"，不是偏好（与第二栏那些开关、大纲表的折叠同一条规矩）。
import { ref } from "vue";

import { t } from "../locales/index.ts";
import type { EditorSession } from "../editor/session.ts";

const props = defineProps<{ session: EditorSession }>();
const { isScene, draft, busy, saved, save } = props.session.scene;

/** 四格的顺序（与核心 `SceneField::ALL` 一致）。 */
const FIELDS = ["pov", "goal", "conflict", "outcome"] as const;

/** 摊着还是收着（开面板时先摊着：默认与从前一样）。切章不重置——同一个组件一直挂着。 */
const open = ref(true);
</script>

<template>
  <form v-if="isScene" class="scene" @submit.prevent="void save()">
    <p class="scene__head">
      <span class="scene__title">{{ t("editor.scene.title") }}</span>
      <!-- 收起来时不摆那行长提示（它是"怎么填"的说明，藏起来才叫真的让开） -->
      <span v-if="open" class="scene__hint">{{ t("editor.scene.hint") }}</span>
      <button
        type="button"
        class="scene__toggle"
        :aria-expanded="open"
        @click="open = !open"
      >
        {{ open ? t("editor.scene.collapse") : t("editor.scene.expand") }}
      </button>
    </p>
    <template v-if="open">
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
    </template>
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
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  opacity: 0.6;
}

/* 收起 / 展开：做成"一行小字里的小机关"，不像个按钮——它不该跟「存下这四格」抢注意力 */
.scene__toggle {
  flex: none;
  margin-left: auto;
  padding: 0 2px;
  border: none;
  background: none;
  color: inherit;
  font: inherit;
  font-size: 11px;
  opacity: 0.7;
  cursor: pointer;
}

.scene__toggle:hover {
  opacity: 1;
  text-decoration: underline;
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
