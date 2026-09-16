<script setup lang="ts">
// 叩问面板：**只问不写**。
//
// 机制挑出来的问题摆到作者面前，他做两件事：**处置**（说好 / 延后 / 舍弃 / 静音 / 记灵感）
// 与**作答**（把心里那一句写下来——进答案池；想的话顺手落进这一章的正文）。
//
// 这个文件只管**长什么样**：会发生什么（挑什么、怎么处置、答案怎么落）全在
// [`useFlowPanel`](./flow-panel.ts) 里——两份的变化理由不一样，别混在一起。
//
// 拆件：一条候选在 `QuestionCard`、正在问的那一张在 `AskingPane`、延后菜单在 `DeferMenu`、
// 记灵感在 `InspireBox`、作答在 `AnswerBox`、回头路在 `FlowRecall`——各有各的变化理由。
import { t } from "../locales/index.ts";
import type { EditorSession } from "../editor/session.ts";
import type { SelectedQuestion } from "../api/question.ts";
import { dueLabel } from "./question.ts";
import { useFlowPanel } from "./flow-panel.ts";
import QuestionCard from "./QuestionCard.vue";
import AnswerBox from "./AnswerBox.vue";
import AskingPane from "./AskingPane.vue";
import DeferMenu from "./DeferMenu.vue";
import FlowRecall from "./FlowRecall.vue";
import InspireBox from "./InspireBox.vue";
import RoundTray from "./RoundTray.vue";

const props = defineProps<{
  workId: number | null;
  session: EditorSession;
  /** 推过来的那张卡（作者点了提示条上的「答一句」） */
  openQuestion?: SelectedQuestion | null;
}>();
const emit = defineEmits<{ opened: [] }>();

const {
  board,
  active,
  busy,
  errorCode,
  deferFor,
  inspireFor,
  answerFor,
  justSaved,
  followChapter,
  roundMode,
  canLand,
  selected,
  chapterName,
  forChapter,
  landing,
  round,
  ask,
  skip,
  openAnswer,
  saveAnswer,
  saveInspiration,
  landLastAnswer,
  landRound,
  toggleWalk,
  toggleRound,
  openDefer,
  closeDefer,
  openInspire,
  closeInspire,
  closeAnswer,
  praise,
  discard,
  muteClass,
  deferCurrent,
  undefer,
  retrieve,
  muteSource,
  unmuteClass,
  unmuteSource,
} = useFlowPanel(props, { onPushedOpened: () => emit("opened") });
</script>

<template>
  <aside class="pane">
    <h2 class="pane__title">{{ t("flow.title") }}</h2>
    <p class="pane__rule">{{ t("flow.rule") }}</p>

    <p v-if="errorCode" class="pane__error">{{ t("flow.error", { code: errorCode }) }}</p>
    <p v-if="busy" class="pane__hint">{{ t("flow.loading") }}</p>

    <!-- 两种跟章走法（同时只能开一种）：排序在核心，界面只把当前章报上去 -->
    <label class="follow">
      <input type="checkbox" :checked="followChapter && !roundMode" @change="toggleWalk" />
      {{ t("flow.follow.toggle") }}
    </label>
    <label class="follow">
      <input type="checkbox" :checked="roundMode" @change="toggleRound" />
      {{ t("flow.round.toggle") }}
    </label>
    <p v-if="followChapter" class="pane__hint">
      {{ t("flow.follow.hint", { chapter: chapterName, count: forChapter }) }}
    </p>

    <!-- 正在问的那一张：处置都在这儿做 -->
    <AskingPane
      v-if="active"
      :card="active"
      :busy="busy"
      :can-skip="followChapter"
      @praise="praise"
      @answer="openAnswer"
      @defer="openDefer"
      @discard="discard"
      @mute-class="muteClass"
      @inspire="openInspire"
      @skip="skip"
    />

    <!-- 先问后排版：这一轮攒下的都在这儿（排序 / 删 / 改字，问够了再一起落） -->
    <RoundTray
      v-if="roundMode"
      :items="round.items.value"
      :busy="busy"
      :previewing="round.previewing.value"
      :can-land="canLand"
      :chapter="chapterName"
      :land-at="landing.landAt.value"
      @move="round.move"
      @remove="round.remove"
      @amend="round.amend"
      @retarget="round.retarget"
      @title="round.title"
      @finish="round.finish"
      @back="round.back"
      @clear="round.clear"
      @land="landRound"
      @update:land-at="landing.setLandAt"
    />

    <!-- 「记下了」这条回执放在面板上固定一处：作答之后那一张就离开"正在问"了，
         回执留在里面会跟着一起消失（作者会以为没记上） -->
    <section v-if="justSaved" class="receipt">
      <p class="saved">{{ justSaved }}</p>
      <div v-if="landing.landableAnswer.value" class="row">
        <button class="link" :disabled="busy" @click="landLastAnswer">
          {{ t("flow.land.now") }}
        </button>
        <span class="hint">{{ landing.landHint.value }}</span>
      </div>
    </section>

    <DeferMenu
      v-if="deferFor && active"
      :busy="busy"
      @confirm="deferCurrent"
      @cancel="closeDefer"
    />

    <AnswerBox
      v-if="answerFor && active"
      :busy="busy"
      :target="landing.landTarget.value"
      :scene-title="landing.sceneTitle.value"
      :land-at="landing.landAt.value"
      :land-on-answer="landing.landOnAnswer.value"
      :can-land="canLand"
      :collecting="roundMode"
      @update:target="landing.setLandTarget"
      @update:scene-title="landing.setSceneTitle"
      @update:land-at="landing.setLandAt"
      @update:land-on-answer="landing.setLandOnAnswer"
      @save="saveAnswer"
      @cancel="closeAnswer"
    />

    <InspireBox
      v-if="inspireFor && active"
      :busy="busy"
      @save="saveInspiration"
      @cancel="closeInspire"
    />

    <!-- 候选：点开一张才算问出 -->
    <section v-if="selected.length > 0">
      <h3 class="sec">{{ t("flow.candidates") }}</h3>
      <QuestionCard
        v-for="item in selected"
        :key="item.card_id"
        :item="item"
        :busy="busy"
        @ask="ask"
        @inspire="openInspire"
      />
    </section>
    <p v-else-if="!busy && board" class="pane__hint">{{ t("flow.empty") }}</p>
    <p v-if="(board?.more ?? 0) > 0" class="pane__hint">
      {{ t("flow.more", { count: board?.more ?? 0 }) }}
    </p>

    <!-- 在等条件的 -->
    <section v-if="(board?.open_deferrals?.length ?? 0) > 0">
      <h3 class="sec">{{ t("flow.waiting") }}</h3>
      <p v-for="item in board?.open_deferrals ?? []" :key="item.id" class="waiting">
        <span v-if="item.kind === 'time'">{{ dueLabel(item.due_at_ms, Date.now()) }}</span>
        <span v-else-if="item.kind === 'written'">{{ t("flow.waiting.written") }}</span>
        <span v-else>{{ t("flow.waiting.manual") }}</span>
        <em v-if="item.note"> · {{ t("flow.waiting.note", { note: item.note }) }}</em>
        <button class="link" :disabled="busy" @click="undefer(item.card_id)">
          {{ t("flow.waiting.cancel") }}
        </button>
      </p>
    </section>

    <!-- 回头路：冷却库（捞回）与两张已静音清单——单独成件，见 `FlowRecall` -->
    <FlowRecall
      :busy="busy"
      :cooled="board?.cooled ?? []"
      :muted-classes="board?.muted_classes ?? []"
      :muted-sources="board?.muted_sources ?? []"
      @retrieve="retrieve"
      @mute-source="muteSource"
      @unmute-class="unmuteClass"
      @unmute-source="unmuteSource"
    />
  </aside>
</template>

<style scoped>
.pane {
  padding: 12px;
  border-left: 1px solid var(--ym-line);
  background: var(--ym-paper-dim);
  overflow: auto;
  font-size: 13px;
}
.pane__title {
  margin: 0 0 4px;
  font-size: 13px;
}
.pane__rule,
.pane__hint {
  margin: 0 0 8px;
  font-size: 11px;
  line-height: 1.5;
  opacity: 0.7;
}
.pane__error {
  margin: 0 0 8px;
  font-size: 11px;
  color: var(--ym-danger, #c0392b);
}
.follow {
  display: flex;
  gap: 4px;
  align-items: center;
  margin: 0 0 4px;
  font-size: 11px;
}
.sec {
  margin: 12px 0 6px;
  font-size: 11px;
  opacity: 0.75;
}
.row {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
  font-size: 11px;
  opacity: 0.85;
}
.link {
  padding: 2px 8px;
  font-size: 11px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: transparent;
  cursor: pointer;
}
.link:disabled {
  opacity: 0.5;
  cursor: default;
}
.waiting {
  margin: 0 0 4px;
  font-size: 12px;
}
.waiting em {
  font-style: normal;
  opacity: 0.7;
}
.receipt {
  margin: 6px 0 0;
}
.saved {
  margin: 0 0 4px;
  font-size: 11px;
  opacity: 0.8;
}
.hint {
  font-size: 11px;
  opacity: 0.7;
}
</style>
