<script setup lang="ts">
// 「大纲」弹窗：**大纲本体的一处收口**——人物 / 设定 / 事件 / 场景卡 / 伏笔。
//
// 为什么收在一处：这些东西本来散在三个地方（人物与伏笔在弹窗里、事件在创作流的碎片池、
// 场景卡四格要打开那一章才填得到），作者得先记住"什么在哪儿"才用得起来——
// 真机反馈就是"缺事件、冲突、结果这些要素"。现在顶上五片页签，一样一页。
//
// 这个文件只管**外壳**（遮罩 + 卡片 + 页签 + 现在露哪一页）：
// 每一页的字段与动作在各自的组件里；读哪一页由 [`useLore`](./lore.ts) 说了算。
import type { EditorSession } from "../editor/session.ts";
import { t } from "../locales/index.ts";
import EntityPane from "./EntityPane.vue";
import EventPane from "./EventPane.vue";
import ForeshadowPane from "./ForeshadowPane.vue";

const props = defineProps<{ session: EditorSession }>();
const { tab, pick, hide } = props.session.lore;

/** 五片页签的顺序（字典键 `lore.tab.*`）：先有人、再有世界、再有发生了什么。 */
const TABS = ["persons", "settings", "events", "foreshadows"] as const;
</script>

<template>
  <div class="all dialog" @click.self="hide()">
    <section class="all__box dialog__box">
      <header class="all__head dialog__head">
        <h2 class="all__title dialog__title">{{ t("lore.title") }}</h2>
        <button type="button" class="dialog__button" @click="hide()">{{ t("common.close") }}</button>
      </header>

      <nav class="all__tabs">
        <button
          v-for="item in TABS"
          :key="item"
          type="button"
          class="all__tab"
          :class="{ 'all__tab--on': tab === item }"
          @click="pick(item)"
        >
          {{ t(`lore.tab.${item}`) }}
        </button>
      </nav>

      <EntityPane v-if="tab === 'persons'" :session="session" kind="person" />
      <EntityPane v-else-if="tab === 'settings'" :session="session" kind="setting" />
      <EventPane v-else-if="tab === 'events'" :session="session" />
      <ForeshadowPane v-else :session="session" />
    </section>
  </div>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./outline-dialog.css"></style>
