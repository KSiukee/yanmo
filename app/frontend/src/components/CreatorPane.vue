<script setup lang="ts">
// 创作流那一栏的**壳**：栏里两片，顶上切一下。
//
// 三片各自成件：[`FragmentsPiece`](./FragmentsPiece.vue)（你自己记下的东西）、
// [`InspirePane`](./InspirePane.vue)（同一批碎片按引力排出来的那一屏）与
// [`CollidePane`](./CollidePane.vue)（挑两条摆一起，你写下撞出来的那句）。壳只管
// "现在露哪一片"——**页签只在栏内切，不动栏顶那两片**（叩问 / 创作流）。
//
// 为什么这么分：那两片一块管"记事的规矩"、一块管"排序的公式"，变化理由完全不同；
// 壳再长上几十行，就变成一份谁也说不清该改哪儿的文件了。
import { ref } from "vue";

import type { EditorSession } from "../editor/session.ts";
import { t } from "../locales/index.ts";
import CollidePane from "./CollidePane.vue";
import FragmentsPiece from "./FragmentsPiece.vue";
import InspirePane from "./InspirePane.vue";
import OutlineCheck from "./OutlineCheck.vue";

defineProps<{ session: EditorSession }>();

/** 栏内现在露哪一片（**不落盘**：这是"当下想看什么"，不是长期偏好） */
const piece = ref<"fragments" | "inspire" | "collide">("fragments");
</script>

<template>
  <aside class="pane">
    <h2 class="pane__title">{{ t("creator.title") }}</h2>
    <p class="pane__rule">{{ t("creator.rule") }}</p>

    <!-- 栏内两片：碎片是你记的，灵感是**同一批碎片排过序**的那一屏 -->
    <nav class="pieces">
      <button
        type="button"
        class="piece"
        :class="{ 'piece--on': piece === 'fragments' }"
        @click="piece = 'fragments'"
      >
        {{ t("creator.piece.fragments") }}
      </button>
      <button
        type="button"
        class="piece"
        :class="{ 'piece--on': piece === 'inspire' }"
        @click="piece = 'inspire'"
      >
        {{ t("creator.piece.inspire") }}
      </button>
      <button
        type="button"
        class="piece"
        :class="{ 'piece--on': piece === 'collide' }"
        @click="piece = 'collide'"
      >
        {{ t("creator.piece.collide") }}
      </button>
    </nav>

    <FragmentsPiece v-if="piece === 'fragments'" :session="session" />
    <!-- 「灵感」那一片：同一批碎片按引力排的一屏（按需读，不打开就不读） -->
    <InspirePane v-else-if="piece === 'inspire'" :session="session" />
    <!-- 「碰撞」那一片：挑两条摆一起，你写下撞出来的那句（按需读） -->
    <CollidePane v-else :session="session" />

    <!-- 大纲体检：**只看不说**——对不上的地方列在这儿，改不改由作者；
         点「去设定卡改」把那一屏打开（两块住同一栏，所以不必再开一栏） -->
    <OutlineCheck
      :session="session"
      @open-entities="(tab) => session.lore.show(tab)"
      @open-foreshadows="session.lore.show('foreshadows')"
    />
  </aside>
</template>

<style scoped>
.pane {
  display: flex;
  flex-direction: column;
  min-height: 0;
  padding: 12px;
  overflow: auto;
  font-size: 13px;
}

.pane__title {
  margin: 0 0 4px;
  font-size: 13px;
}

.pane__rule {
  margin: 0 0 8px;
  font-size: 11px;
  line-height: 1.5;
  opacity: 0.7;
}

/* 栏内那两片：**只在这一栏里切**，不动栏顶那两片页签 */
.pieces {
  display: flex;
  gap: 4px;
  margin: 0 0 8px;
}

.piece {
  padding: 2px 10px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: transparent;
  color: inherit;
  font: inherit;
  font-size: 11px;
  cursor: pointer;
}

.piece--on {
  border-color: var(--ym-accent);
  color: var(--ym-accent);
}
</style>
