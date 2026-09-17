<script setup lang="ts">
// 「计划 vs 实际」：**每一章的计划，与正文里字面认得到的东西对一遍**。
//
// 认的是字面：设定卡的名字 / 别称有没有出现在正文里；还埋着的伏笔有没有相近说法。
// 四格与「一句话」判不了（那要实体抽取）——它们只并排摆着看，**不判对错**。
//
// 三条纪律写在这儿：
// 1. **永远显示差异视图**：计划一列、实际一列、差在哪一条一条摆出来，不直接改稿；
// 2. **只补不删**：「正文里出现、计划里没有」的人可以一键补进计划；计划里挂了、
//    正文里没认到的人**只提示**（删计划比加计划危险得多）；
// 3. 补之前核心会先给这一章的大纲留一份底，所以「撤销」一直都在。
import { computed, ref, watch } from "vue";

import { t } from "../locales/index.ts";
import {
  outlineActuals,
  outlineAlignCast,
  outlineAlignUndo,
  type ActualCastRef,
  type ChapterActualDto,
  type OutlineActualPageDto,
} from "../api/outline";
import { alignTargets, stateAfterAlign, stateLabelKey, undoRef } from "./actual.ts";
import type { EditorSession } from "../editor/session.ts";

const props = defineProps<{ session: EditorSession }>();
const workId = computed(() => props.session.workId.value);

const page = ref<OutlineActualPageDto | null>(null);
const busy = ref(false);
const errorText = ref<string | null>(null);
/** 展不展开哪一章的明细（默认都收起：先看有问题的，再看细节） */
const opened = ref<number[]>([]);

function describe(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

async function scan(): Promise<void> {
  const work = workId.value;
  if (work === null) {
    page.value = null;
    return;
  }
  busy.value = true;
  errorText.value = null;
  try {
    page.value = await outlineActuals(work);
  } catch (error) {
    errorText.value = describe(error);
  } finally {
    busy.value = false;
  }
}

/** 名字之间的顿号是**界面文案**，从字典取（组件里不写标点）。 */
const nameSep = t("actual.name_sep");

/** 把一串卡片拼成人看的一行（空的时候给"（空）"，不是空白）。 */
function names(list: ActualCastRef[], pick: (item: ActualCastRef) => string = (item) => item.name): string {
  return list.length > 0 ? list.map(pick).join(nameSep) : t("actual.none");
}

/** 伏笔候选那一行的说法（含括号与分隔符——都在字典里）。 */
function foreshadowLine(hit: { body: string; fragments: string[] }): string {
  return t("actual.foreshadow_item", {
    body: hit.body,
    fragments: hit.fragments.join(t("actual.fragment_sep")),
  });
}

/** 只看有问题的那些（已写好的不占地方） */
const shown = computed(() =>
  (page.value?.chapters ?? []).filter((chapter) => chapter.state !== "written"),
);
const written = computed(() =>
  (page.value?.chapters ?? []).filter((chapter) => chapter.state === "written"),
);

function toggle(node_id: number): void {
  opened.value = opened.value.includes(node_id)
    ? opened.value.filter((id) => id !== node_id)
    : [...opened.value, node_id];
}

/** 补：把"正文里出现、计划里没有"的人补进这一章的出场人物（服务端只补不删）。 */
async function align(chapter: ChapterActualDto): Promise<void> {
  const ids = alignTargets(chapter);
  if (ids.length === 0) return;
  busy.value = true;
  errorText.value = null;
  try {
    const receipt = await outlineAlignCast(chapter.node_id, ids);
    const node_id = chapter.node_id;
    const next = stateAfterAlign(chapter);
    const current = page.value;
    if (!current) return;
    // 本地把这一条改成"新计划"（点完立刻看得见）；**下一次全书对一遍才是准的**
    page.value = {
      ...current,
      chapters: current.chapters.map((item) =>
        item.node_id === node_id
          ? {
              ...item,
              state: next,
              planned: receipt.cast.map((member) => ({
                entity_id: member.entity_id,
                name: member.name,
                matched: member.name,
              })),
              matched: [...item.matched, ...item.extra],
              extra: [],
            }
          : item,
      ),
      undoable: receipt.snapshot_id === null
        ? current.undoable
        : [
            ...current.undoable.filter((item) => item.node_id !== node_id),
            { id: receipt.snapshot_id, node_id, note: "align_cast", created_at: Date.now() },
          ],
    };
  } catch (error) {
    errorText.value = describe(error);
  } finally {
    busy.value = false;
  }
}

/** 撤销上一次对齐：把这一章的大纲放回最近那份留底。 */
async function undo(chapter: ChapterActualDto): Promise<void> {
  busy.value = true;
  errorText.value = null;
  try {
    await outlineAlignUndo(chapter.node_id);
    await scan();
  } catch (error) {
    errorText.value = describe(error);
  } finally {
    busy.value = false;
  }
}

watch(workId, () => void scan(), { immediate: true });
</script>

<template>
  <p class="grid__rule">{{ t("actual.rule") }}</p>
  <p v-if="errorText" class="grid__error">{{ errorText }}</p>
  <p v-if="workId === null" class="grid__hint">{{ t("grid.no_work") }}</p>

  <nav class="grid__columns">
    <button type="button" class="dialog__button" :disabled="busy || workId === null" @click="void scan()">
      {{ busy ? t("actual.scanning") : t("actual.scan") }}
    </button>
    <span class="grid__hint">{{ t("actual.hint") }}</span>
  </nav>

  <p v-if="page && page.truncated > 0" class="grid__hint">
    {{ t("actual.truncated", { count: page.truncated }) }}
  </p>
  <p v-if="page && shown.length === 0 && written.length > 0" class="grid__hint">
    {{ t("actual.all_written", { count: written.length }) }}
  </p>
  <p v-else-if="page && page.chapters.length === 0" class="grid__hint">{{ t("actual.nothing") }}</p>

  <div v-if="shown.length > 0" class="grid__scroll">
    <ul class="actual__list">
      <li v-for="chapter in shown" :key="chapter.node_id" class="actual__row">
        <header class="actual__head" @click="toggle(chapter.node_id)">
          <span class="actual__state" :class="`actual__state--${chapter.state}`">
            {{ t(stateLabelKey(chapter.state)) }}
          </span>
          <span class="actual__title">{{ chapter.title || t("common.untitled") }}</span>
          <span class="actual__counts">
            {{ t("actual.counts", { extra: chapter.extra.length, missing: chapter.missing.length }) }}
          </span>
        </header>

        <div v-if="opened.includes(chapter.node_id)" class="actual__detail">
          <p class="actual__line">
            <span class="actual__label">{{ t("actual.planned") }}</span>
            <span>{{ names(chapter.planned) }}</span>
          </p>
          <p class="actual__line">
            <span class="actual__label">{{ t("actual.matched") }}</span>
            <span>{{ names(chapter.matched, (cast) => cast.matched) }}</span>
          </p>
          <p v-if="chapter.extra.length > 0" class="actual__line">
            <span class="actual__label">{{ t("actual.extra") }}</span>
            <span>{{ names(chapter.extra) }}</span>
          </p>
          <p v-if="chapter.missing.length > 0" class="actual__line actual__line--warn">
            <span class="actual__label">{{ t("actual.missing") }}</span>
            <span>{{ names(chapter.missing) }}</span>
          </p>
          <p v-if="chapter.foreshadows.length > 0" class="actual__line">
            <span class="actual__label">{{ t("actual.foreshadow") }}</span>
            <span>
              <span v-for="hit in chapter.foreshadows" :key="hit.foreshadow_id" class="actual__hit">
                {{ foreshadowLine(hit) }}
              </span>
            </span>
          </p>
          <p class="actual__line actual__hint">{{ t("actual.field_hint") }}</p>

          <div class="actual__actions">
            <button
              v-if="chapter.extra.length > 0"
              type="button"
              class="dialog__button"
              :disabled="busy"
              @click="void align(chapter)"
            >
              {{ t("actual.align", { count: chapter.extra.length }) }}
            </button>
            <button
              v-if="undoRef(page, chapter.node_id)"
              type="button"
              class="dialog__button"
              :disabled="busy"
              @click="void undo(chapter)"
            >
              {{ t("actual.undo") }}
            </button>
          </div>
        </div>
      </li>
    </ul>
  </div>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./outline-grid.css"></style>
<style scoped src="./actual.css"></style>
