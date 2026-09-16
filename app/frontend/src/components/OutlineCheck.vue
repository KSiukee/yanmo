<script setup lang="ts">
// 大纲体检（创作流面板里的那一块）：**只看不说**——把对不上的地方列出来。
//
// 为什么放在创作流那一栏：设计里「创作流」本来就是"你自己记的东西 + 机制替你看着的东西"
// （冲突提醒与灵感速记同一栏）；另开一栏会把窗口挤成四列。
//
// 这个文件只管长什么样；状态与命令在 [`useOutlinePanel`](./outline-panel.ts)，
// 规则在核心（`outline::rules`，纯逻辑、零文案）。
import { t } from "../locales/index.ts";
import type { EditorSession } from "../editor/session.ts";
import { anchorTarget, issueText, missingLine } from "./outline.ts";
import { useOutlinePanel } from "./outline-panel.ts";

const props = defineProps<{ session: EditorSession }>();
const emit = defineEmits<{
  openEntities: ["persons" | "settings"];
  openForeshadows: [];
}>();

const { workId, directory } = props.session;
const { board, busy, errorText, showKnown, open, known, refresh, dismiss, undismiss, clearDismissed } =
  useOutlinePanel({ workId });

/** 这一条的锚点指向哪儿（决定摆哪个按钮：去设定卡 / 跳到这一场）。 */
function target(anchors: string[]) {
  return anchorTarget(anchors);
}

/**
 * 这条实体问题该打开「大纲」的哪一页。
 *
 * 核心在参数里给了卡的类型（`kind`）：人物那条就开人物页、设定那条就开设定页——
 * 不然作者点「去大纲里改」还得自己再找一遍。
 */
function entityTab(issue: { params: Record<string, string> }): "persons" | "settings" {
  return issue.params.kind === "setting" ? "settings" : "persons";
}

/** 跳到这一场：先在树上把它露出来（卷可能是收着的），再切过去。 */
async function goScene(node_id: number) {
  await directory.reveal(node_id);
  await directory.select(node_id);
}
</script>

<template>
  <section class="check">
    <h3 class="check__head">
      <span class="check__title">{{ t("outline.title") }}</span>
      <button type="button" class="check__link" :disabled="busy" @click="void refresh()">
        {{ t("outline.recheck") }}
      </button>
    </h3>
    <p class="check__rule">{{ t("outline.rule") }}</p>
    <p v-if="errorText" class="check__error">{{ errorText }}</p>

    <template v-if="board">
      <p v-if="open.length > 0" class="check__count">
        {{ t("outline.count", { count: open.length }) }}
      </p>
      <ul v-if="open.length > 0" class="check__list">
        <li v-for="issue in open" :key="issue.fingerprint" class="check__item">
          <p class="check__text">{{ issueText(issue) }}</p>
          <p v-if="missingLine(issue)" class="check__missing">{{ missingLine(issue) }}</p>
          <div class="check__row">
            <button
              v-if="target(issue.anchors).kind === 'entity'"
              type="button"
              class="check__link"
              :disabled="busy"
              @click="emit('openEntities', entityTab(issue))"
            >
              {{ t("outline.go_entities") }}
            </button>
            <button
              v-else-if="target(issue.anchors).kind === 'scene'"
              type="button"
              class="check__link"
              :disabled="busy"
              @click="void goScene(target(issue.anchors).id!)"
            >
              {{ t("outline.go_scene") }}
            </button>
            <button
              v-else-if="target(issue.anchors).kind === 'foreshadow'"
              type="button"
              class="check__link"
              :disabled="busy"
              @click="emit('openForeshadows')"
            >
              {{ t("outline.go_foreshadows") }}
            </button>
            <button type="button" class="check__link" :disabled="busy" @click="dismiss(issue)">
              {{ t("outline.dismiss") }}
            </button>
          </div>
        </li>
      </ul>
      <p v-else-if="!busy" class="check__hint">{{ t("outline.empty") }}</p>

      <!-- 回头路：忽略过的能列出来、能捡回来（忽略不是销毁） -->
      <p v-if="known.length > 0" class="check__row">
        <button type="button" class="check__link" :disabled="busy" @click="showKnown = !showKnown">
          {{ showKnown ? t("outline.hide_dismissed") : t("outline.show_dismissed") }}
        </button>
        <span class="check__hint">{{ t("outline.dismissed", { count: known.length }) }}</span>
        <button type="button" class="check__link" :disabled="busy" @click="clearDismissed()">
          {{ t("outline.clear_dismissed") }}
        </button>
      </p>
      <ul v-if="showKnown" class="check__list check__list--known">
        <li v-for="issue in known" :key="issue.fingerprint" class="check__item">
          <p class="check__text">{{ issueText(issue) }}</p>
          <button type="button" class="check__link" :disabled="busy" @click="undismiss(issue)">
            {{ t("outline.undismiss") }}
          </button>
        </li>
      </ul>
    </template>
    <p v-else-if="busy" class="check__hint">{{ t("outline.loading") }}</p>
  </section>
</template>

<style scoped>
.check {
  margin: 12px 0 0;
  padding: 10px 0 0;
  border-top: 1px solid var(--ym-line);
}

.check__head {
  display: flex;
  gap: 8px;
  align-items: baseline;
  margin: 0 0 4px;
  font-size: 11px;
}

.check__title {
  letter-spacing: 0.08em;
  opacity: 0.85;
}

.check__rule,
.check__hint,
.check__missing {
  margin: 0 0 6px;
  font-size: 11px;
  line-height: 1.5;
  opacity: 0.7;
}

.check__error {
  margin: 0 0 6px;
  font-size: 11px;
  color: var(--ym-danger, #c0392b);
}

.check__count {
  margin: 0 0 4px;
  font-size: 11px;
  opacity: 0.8;
}

.check__list {
  margin: 0 0 6px;
  padding: 0;
  list-style: none;
}

.check__list--known {
  opacity: 0.7;
}

.check__item {
  margin: 0 0 8px;
  padding: 0 0 6px;
  border-bottom: 1px solid var(--ym-line);
}

.check__text {
  margin: 0 0 4px;
  font-size: 12px;
  line-height: 1.5;
}

.check__row {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
  margin: 0;
}

.check__link {
  padding: 1px 8px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: transparent;
  color: inherit;
  font: inherit;
  font-size: 11px;
  cursor: pointer;
}

.check__link:disabled {
  opacity: 0.5;
  cursor: default;
}
</style>
