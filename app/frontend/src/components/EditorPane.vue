<script setup lang="ts">
// 正文编辑器面板：**只挂当前章**（边写边存 + 切章 + 关窗闸门都在 editor/session.ts 里）。
//
// 这个文件只负责"长什么样"：会话编排是另一个变化理由，别混在一起。
// 会话由布局层建一次（目录树与编辑器说的是同一本书、同一章），这里只把它摊开。
import { computed, nextTick, ref } from "vue";
import { EditorContent } from "@tiptap/vue-3";

import { t } from "../locales/index.ts";
import {
  caliberLabelKey,
  countUnitKey,
  languageLabelKey,
  pickCount,
} from "../editor/wordcount.ts";
import { goalProgress, goalReached, pickDayCount } from "../editor/writing-days.ts";
import { sourceHighlights, sourceLabelKey, sourceRows } from "../editor/source-view.ts";
import { editorSurface } from "../editor/pane-style.ts";
import { shortcutKeys } from "../editor/shortcuts.ts";
import { useSplitAction } from "./split-action.ts";
import type { EditorSession } from "../editor/session";
import type { AutosaveState } from "../editor/autosave";
import ExitDialog from "./ExitDialog.vue";
import SceneFields from "./SceneFields.vue";
import { looksLikeFirstRun } from "../editor/shelf";
import { secondsText } from "../editor/drift.ts";
import { stallStats } from "../editor/stall-watch.ts";
import SplitDialog from "./SplitDialog.vue";

const props = defineProps<{ session: EditorSession }>();
// 摊开之后每一项都是顶层绑定，模板里照常自动解包
const {
  editor,
  chapterTitle,
  saveState,
  sources,
  exitState,
  failure,
  crashNotice,
  retryExit,
  escapeExit,
  forceExit,
  language,
  caliber,
  cycleCaliber,
  cycleLanguage,
  switching,
} = props.session;

// 专注模式：退出按钮就挂在章名这一行——专注时它是屏幕上唯一的"出口"，
// 藏起来就等于把作者关在里面了（快捷键是捷径，不能是唯一的路）。
const { on: zenOn, exit: exitZen } = props.session.zen;
// 专注 / 全屏时**放宽阅读行宽**：窗口变宽了，正文还卡在 42em 会白掉一大片。
// 也不是无限拉——上限 64em（中文长文再宽，眼睛回行容易串行）。
const fullscreenOn = props.session.fullscreenOn;
const wideMeasure = computed(() => zenOn.value || fullscreenOn.value);

// 正文排版（字号 / 行距 / 字距）、稿纸横线与正文背景：**显示层**，绑成 CSS 变量交给样式表用。
// 取"这本书生效的那一份"（每书覆盖优先），没打开书就退回全局；档位与换算全在
// editor/typography.ts / rule.ts / background.ts 那一处——组件里不写死 px、倍数或颜色。
// 三样**合起来**那一步在 editor/pane-style.ts（这一件贴着 400 行的棘轮，算法不往这儿塞）。
const display = computed(
  () => props.session.appearance.workValues.value ?? props.session.appearance.values.value,
);
// ★ 图那一段是壳在设置面板里读回来的（`appearance.background`）：
//   组件手里只有渲染用的 data URL，**永远没有路径**。
const surface = computed(() =>
  editorSurface(display.value, props.session.appearance.background.value),
);

// 今日进度：账本在核心（`writing_days`），会话在每次落盘成功后刷一次。
// 没设目标就只显示"今天写了多少"——不拿一个假目标糊弄作者。
const {
  today: todayWriting,
  goal: dailyGoal,
  open: openWriting,
} = props.session.writing;
const todayCount = computed(() => pickDayCount(todayWriting.value, caliber.value));
const todayProgress = computed(() => goalProgress(todayCount.value, dailyGoal.value));
const todayReached = computed(() => goalReached(todayCount.value, dailyGoal.value));
const todayText = computed(() =>
  dailyGoal.value === null
    ? t("editor.today", { count: todayCount.value })
    : t("editor.today_of_goal", { count: todayCount.value, goal: dailyGoal.value }),
);

// 状态栏那个数字：三个口径都在手上（落盘时一起回来），**点一下就换一个**。
// 口径名与单位都从字典取（核心只给码，文案在 locales 里）。
const countText = computed(() =>
  t(countUnitKey(caliber.value), { count: pickCount(saveState.value, caliber.value) }),
);
const countTitle = computed(() =>
  t("editor.caliber.switch_title", { name: t(caliberLabelKey(caliber.value)) }),
);
// 来源读数：**只露不是「键入」的那几档**——打字是常态，不必提醒；
// 真正值得一眼看见的是"其中有多少不是打出来的"（粘贴 / 模块 / 未记录）。
// 全部都是键入时这一格不出现（不占位置，也不制造"一切正常"的噪声）。
const sourceLines = computed(() => sourceHighlights(sources.value, caliber.value));
const sourceHint = computed(() =>
  sourceLines.value.length === 0
    ? ""
    : t("editor.source.hint", { parts: sourceParts(sourceLines.value) }),
);
const sourceTitle = computed(() => {
  const rows = sourceRows(sources.value, caliber.value);
  return rows.length === 0
    ? ""
    : t("editor.source.title", {
        caliber: t(caliberLabelKey(caliber.value)),
        parts: sourceParts(rows),
      });
});

/** 「键入 900 · 粘贴 300」——单位与档名都从字典取，组件里不拼中文。 */
function sourceParts(lines: { source: string; count: number }[]): string {
  return lines.map((line) => `${t(sourceLabelKey(line.source))} ${line.count}`).join(" · ");
}
const languageText = computed(() => t(languageLabelKey(language.value)));
const languageTitle = computed(() =>
  t("editor.language.switch_title", { name: languageText.value }),
);
// 版本历史：跟当前章绑在一起，入口就在章名这一行（状态机在 editor/snapshots.ts）
const { toggle: toggleSnapshots } = props.session.snapshots;

// 首启那条提示：核心在首启会先建一本**无名空壳**，所以作者看到的是"一片空白 + 未命名作品"。
// 判据（只有一本、没名字、一个字没写）放在 `shelf.looksLikeFirstRun` 里——机械、可单测，
// 而且作者只要写了一个字、或者建了第二本，它自己就消失，不需要另记"看过引导没有"。
// 这条提示上挂**两个**入口：另建一本，或者**直接给这本空壳起个名**（不另建）。
const firstRun = computed(
  () =>
    looksLikeFirstRun(props.session.shelf.entries.value) &&
    !props.session.shelf.visible.value &&
    !props.session.shelf.form.value,
);

// 卡顿读数：**界面被占住过多久**——不是"稿子出问题了"，所以它只是一个读数，
// 不弹窗、不打断、也不做任何动作。每秒量一次（见 editor/stall-watch.ts）。
const stallText = computed(() =>
  stallStats.value.count === 0
    ? ""
    : t("editor.stall", {
        count: stallStats.value.count,
        seconds: secondsText(stallStats.value.maxMs),
      }),
);
const stallTitle = computed(() =>
  t("editor.stall_title", {
    count: stallStats.value.count,
    max: secondsText(stallStats.value.maxMs),
    total: secondsText(stallStats.value.totalMs),
  }),
);
// 排版清理：同一条思路——一次动作、只作用于当前章（状态机在 editor/typeset.ts）
const { open: openTypeset } = props.session.typeset;
// 这一章的"一句话"（投稿包的大纲要用它）：**单独存、单独显示**，不跟正文搅在一起
const {
  value: noteValue,
  draft: noteDraft,
  editing: noteEditing,
  busy: noteBusy,
  open: openNote,
  save: saveNote,
  cancel: cancelNote,
} = props.session.note;
const noteEl = ref<HTMLInputElement | null>(null);

/** 点开"一句话"：打开输入框并聚焦（像目录树改名那样，点开就写，不打断写作） */
async function startNote() {
  openNote();
  await nextTick();
  noteEl.value?.focus();
}

// 状态 → 字典键：文案在 locales 里，这里只留"哪个状态对应哪句话"

// ── 拆章：光标放在要分出去的那一段上，点一下，确认了才动 ──
//
// 状态与那几条口径住在 `components/split-action.ts`（"光标一动重算一次"是**行为**，
// 与版式不是同一个变化理由）；这里只用它，并把真去拆那一下交给会话。
const {
  canSplitHere,
  dialog: splitPoint,
  hint: splitHint,
  busy: splitBusy,
  open: openSplit,
  confirm: confirmSplit,
} = useSplitAction(props.session.editor, (at, title) => props.session.splitChapterAt(at, title));

// 状态 → 字典键：文案在 locales 里，这里只留"哪个状态对应哪句话"
const STATUS_KEYS: Record<AutosaveState["status"], string> = {
  idle: "editor.status.idle",
  pending: "editor.status.pending",
  saving: "editor.status.saving",
  saved: "editor.status.saved",
  error: "editor.status.error",
  desync: "editor.status.desync",
};

function statusText(status: AutosaveState["status"]): string {
  return t(STATUS_KEYS[status]);
}
</script>

<template>
  <section class="editor">
    <header class="editor__bar">
      <span class="editor__title">{{ chapterTitle || t("editor.untitled") }}</span>
      <button
        v-if="zenOn"
        type="button"
        class="editor__versions"
        :title="t('editor.zen_exit_title', { key: shortcutKeys('exit-focus') })"
        @click="exitZen()"
      >
        {{ t("editor.zen_exit") }}
      </button>
      <button
        type="button"
        class="editor__versions"
        :title="t('snapshots.open_title')"
        @click="toggleSnapshots()"
      >
        {{ t("snapshots.button") }}
      </button>
      <button
        type="button"
        class="editor__versions"
        :title="t('typeset.button_title')"
        @click="void openTypeset()"
      >
        {{ t("typeset.button") }}
      </button>
      <button
        type="button"
        class="editor__versions"
        :title="t('editor.note_title')"
        @click="void startNote()"
      >
        {{ t("editor.note_button") }}
      </button>
      <button
        type="button"
        class="editor__versions"
        :title="canSplitHere ? t('split.button_title') : t('split.cannot')"
        :disabled="!canSplitHere || switching"
        @click="openSplit()"
      >
        {{ t("split.button") }}
      </button>
      <span class="editor__meta">
        <span v-if="failure" class="editor__bad" :title="failure">{{ failure }}</span>
        <template v-else>
          <button
            type="button"
            class="editor__count"
            :title="countTitle"
            @click="cycleCaliber()"
          >
            {{ countText }}
          </button>
          <!-- 来源读数：只有真出现「不是打出来的字」时才在状态栏露一行。
               悬停能看到完整分布（含键入），这里只露要留意的几档。 -->
          <span v-if="sourceHint" class="editor__sources" :title="sourceTitle">
            {{ sourceHint }}
          </span>
          <button
            type="button"
            class="editor__lang"
            :title="languageTitle"
            @click="cycleLanguage()"
          >
            {{ languageText }}
          </button>
          <!-- 今日进度：点一下开码字日历。进度条只在"还没达标"时画——满格的条是一条
               没有信息的常驻占位（真机提的）；达标后它在**同一个槽位**里换成一个短记号，
               于是达标那一刻整条栏不会重排。宽度不钉死：它只在进位数/设目标时变宽，
               钉了反而会在格与格之间撑出空白 -->
          <button
            type="button"
            class="editor__today"
            :title="t('editor.today_title')"
            @click="void openWriting()"
          >
            <span class="editor__today-num" :class="{ 'editor__today-num--done': todayReached }">
              {{ todayText }}
            </span>
            <span
              v-if="todayProgress !== null"
              class="editor__today-bar"
              :class="{ 'editor__today-bar--done': todayReached }"
              :title="todayReached ? t('editor.today_reached') : undefined"
            >
              <span
                v-if="!todayReached"
                class="editor__today-fill"
                :style="{ width: `${Math.round(todayProgress * 100)}%` }"
              />
              <template v-else>{{ t("editor.today_done") }}</template>
            </span>
          </button>
          <span
            class="editor__status"
            :class="`editor__status--${saveState.status}`"
            :title="saveState.detail"
          >
            {{ statusText(saveState.status) }}
          </span>
          <!-- 卡顿读数：**只有真卡过才出现**——它是一份事实，不是待办事项，
               所以不提醒、不可点、不占位置（没卡过时这条根本没有）。 -->
          <span v-if="stallText" class="editor__stall" :title="stallTitle">
            {{ stallText }}
          </span>
        </template>
      </span>
    </header>

    <p v-if="firstRun" class="editor__notice editor__notice--first">
      {{ t("shelf.first_run_notice") }}
      <button
        type="button"
        class="editor__notice-action"
        @click="props.session.shelf.openCreate()"
      >
        {{ t("shelf.first_run_action") }}
      </button>
      <button
        type="button"
        class="editor__notice-action"
        @click="props.session.shelf.openAdoptShell()"
      >
        {{ t("shelf.first_run_action_here") }}
      </button>
    </p>
    <p v-if="crashNotice" class="editor__notice">⚠ {{ crashNotice }}</p>
    <p v-if="saveState.incident" class="editor__notice">
      {{ t("editor.incident", { kind: saveState.incident }) }}
    </p>

    <!-- 拆章的两条交代：**切不了时说清为什么**；拆完给一次撤销的回头路。
         这两条都不是提醒（不催、不阻塞）：一条是"这一下为什么没动"，一条是"刚才那一下怎么退回去"。 -->
    <p v-if="splitHint" class="editor__notice">{{ splitHint }}</p>
    <p v-if="props.session.splitNotice.value" class="editor__notice">
      {{ t("split.done", {
        kept: props.session.splitNotice.value.kept_chars,
        moved: props.session.splitNotice.value.moved_chars,
        name: props.session.splitNotice.value.new_title,
      }) }}
      <button type="button" class="editor__notice-action" :disabled="switching" @click="void props.session.undoSplit()">
        {{ t("split.undo") }}
      </button>
    </p>

    <!-- 这一章的"一句话"：写入框就在正文上方，存了就显示一行，点它接着改 -->
    <form v-if="noteEditing" class="editor__note" @submit.prevent="void saveNote()">
      <input
        ref="noteEl"
        v-model="noteDraft"
        class="editor__note-input"
        type="text"
        :placeholder="t('editor.note_placeholder')"
        @keydown.esc="cancelNote()"
      />
      <button type="submit" class="editor__note-button" :disabled="noteBusy">
        {{ t("common.save") }}
      </button>
      <button type="button" class="editor__note-button" @click="cancelNote()">
        {{ t("common.cancel") }}
      </button>
    </form>
    <p
      v-else-if="noteValue"
      class="editor__note-line"
      :title="t('editor.note_title')"
      @click="void startNote()"
    >
      <span class="editor__note-label">{{ t("editor.note_label") }}</span>{{ noteValue }}
    </p>

    <!-- 场景卡才有的一小张表单：与"一句话"并排，同样单独存（不是场景卡就整块不出现） -->
    <SceneFields :session="session" />

    <EditorContent
      v-if="editor"
      :editor="editor"
      class="editor__area"
      :class="{ 'editor__area--wide': wideMeasure }"
      :data-rule="surface.rule"
      :data-bg="surface.bg"
      :data-bg-layout="surface.bgLayout"
      :style="surface.style"
    />

    <ExitDialog
      v-if="exitState.blocked"
      :message="exitState.message"
      :escape-path="exitState.escapePath"
      :busy="exitState.busy"
      @retry="retryExit"
      @escape="escapeExit"
      @force="forceExit"
    />

    <SplitDialog
      v-if="splitPoint"
      :split="splitPoint"
      :busy="splitBusy"
      @cancel="splitPoint = null"
      @confirm="void confirmSplit($event)"
    />
  </section>
</template>

<style scoped src="./editor-pane.css"></style>
