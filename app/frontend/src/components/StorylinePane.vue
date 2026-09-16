<script setup lang="ts">
// 故事总纲：**整本书讲什么**——一段自由文本（立意 / 主线 / 卖点 / 结局往哪儿走）。
//
// 为什么要这一页（真机一问"现在的大纲没有总纲吗"）：大纲表管的是"**这一章**写什么"，
// 而作者动笔前要想清楚的是"**这本书**讲什么"——橙瓜 / 番茄那一类工具也是把这一层
// 做成一段带模板提示的文字（见 `docs/调研-大纲录入形态.md`）。
//
// 手感与大纲表同一套：**在框里直接写、点别处就存下了**——一段长文没有一个"提交"的时刻。
// 这一页只管长什么样；状态与命令在 [`useStorylinePanel`](./storyline-panel.ts)。
import type { EditorSession } from "../editor/session.ts";
import { t } from "../locales/index.ts";
import { storylineEmpty } from "./storyline.ts";

const props = defineProps<{ session: EditorSession }>();
const { text, busy, errorText, justSaved, save } = props.session.storyline;
</script>

<template>
  <section class="story">
    <p class="story__rule">{{ t("storyline.rule") }}</p>
    <p v-if="errorText" class="story__error">{{ errorText }}</p>

    <textarea
      v-model="text"
      class="story__box"
      :disabled="busy"
      :placeholder="t('storyline.placeholder')"
      :aria-label="t('storyline.title')"
      @blur="void save()"
    ></textarea>

    <p class="story__hint">
      <span v-if="justSaved">{{ justSaved }}</span>
      <span v-else-if="storylineEmpty(text)">{{ t("storyline.empty_hint") }}</span>
      <span v-else>{{ t("storyline.edit_hint") }}</span>
    </p>
  </section>
</template>

<style scoped src="./storyline.css"></style>
