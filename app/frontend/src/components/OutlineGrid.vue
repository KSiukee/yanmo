<script setup lang="ts">
// 「大纲」这一屏的**外壳**：遮罩 + 标题 + 三片页签（总纲 / 章纲 / 计划vs实际）+ Esc 分层退。
//
// 为什么要有两页（0.68.1 按真机反馈改的）：作者点「大纲」想看的先是"这本书讲什么"
// （总纲），再是"每章怎么落"（章纲那张大表）。总纲原先藏在「资料」弹窗的第一页，
// 等于把最该先看的东西放到第三个入口后面——挪到这儿，打开就在眼前。
//
// 三页各自成件（[`StorylinePane`](./StorylinePane.vue) / [`OutlineTable`](./OutlineTable.vue) /
// [`OutlineActual`](./OutlineActual.vue)）：这个文件只管"现在露哪一页、Esc 该怎么退"，
// 别的什么都不碰。
//
// Esc 分三层退（与专注模式那套同一条分寸）：选人卡 → 从章纲回总纲 → 收起整屏。
import type { EditorSession } from "../editor/session.ts";
import { t } from "../locales/index.ts";
import { DEFAULT_PANE, type GridPane } from "./grid.ts";
import OutlineActual from "./OutlineActual.vue";
import OutlineTable from "./OutlineTable.vue";
import StorylinePane from "./StorylinePane.vue";

const props = defineProps<{ session: EditorSession }>();
const { visible, pane, pickPane, castFor, closeCast, hide } = props.session.grid;

/** 三片页签（稳定码 → 字典 `grid.tab.*`）：默认落在总纲。 */
const PANES: GridPane[] = ["storyline", "chapters", "actual"];

function onEscape() {
  if (castFor.value !== null) {
    closeCast();
    return;
  }
  if (pane.value !== DEFAULT_PANE) {
    pickPane(DEFAULT_PANE);
    return;
  }
  hide();
}
</script>

<template>
  <div v-if="visible" class="grid dialog" @click.self="hide()">
    <section class="grid__box dialog__box" @keydown.esc.stop="onEscape()">
      <header class="grid__head dialog__head">
        <h2 class="grid__title dialog__title">{{ t("grid.title") }}</h2>
        <!-- 三片页签：总纲（默认）· 章纲 · 计划vs实际 -->
        <nav class="grid__tabs">
          <button
            v-for="item in PANES"
            :key="item"
            type="button"
            class="grid__tab"
            :class="{ 'grid__tab--on': pane === item }"
            @click="pickPane(item)"
          >
            {{ t(`grid.tab.${item}`) }}
          </button>
        </nav>
        <button type="button" class="dialog__button" @click="hide()">{{ t("common.close") }}</button>
      </header>

      <!-- 第一片：总纲（整本书讲什么）——一段自由文本，框里直接写、点别处就存 -->
      <StorylinePane v-if="pane === 'storyline'" :session="session" />
      <!-- 第二片：章纲（那一张大表） -->
      <OutlineTable v-else-if="pane === 'chapters'" :session="session" />
      <!-- 第三片：计划 vs 实际（正文里认得到什么） -->
      <OutlineActual v-else :session="session" />
    </section>
  </div>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./outline-grid.css"></style>
