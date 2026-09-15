<script setup lang="ts">
// 叩问面板：**只问不写**。
//
// 机制挑出来的问题摆到作者面前，他只做处置：说好（这个问题好）、延后（什么时候再问我）、
// 舍弃（进冷却库）、静音（这类/这个来源）、记灵感（被勾起的念头；记完回到原问题，状态不变）。
//
// 三条界面纪律：
// 1. **点开一张才算"问出"**——那一刻才消耗新颖度；只看列表不算已问；
// 2. 没有"作答"入口：答案输入是下一步的事，宁可没有也不放按不动的按钮；
// 3. 界面不拼句子：模板句从字典渲染（`question.template.*`），核心只给键与槽位。
//
// 拆件：一条候选在 `QuestionCard`、延后菜单在 `DeferMenu`、记灵感在 `InspireBox`——
// 它们各有各的变化理由，堆在一个文件里只会越长越难改。
import { computed, onMounted, onUnmounted, ref, watch } from "vue";

import { t } from "../locales/index.ts";
import { asError } from "../api/errors.ts";
import {
  questionAsk,
  questionBoard,
  questionDefer,
  questionDiscard,
  questionDrafts,
  questionInspire,
  questionMuteClass,
  questionMuteSource,
  questionPraise,
  questionRequeueDue,
  questionRetrieve,
  questionSync,
  questionUndefer,
  questionUnmuteClass,
  questionUnmuteSource,
  type QuestionBoard,
  type SelectedQuestion,
} from "../api/question.ts";
import { classLabel, dueLabel, renderDraft, sourceLabel } from "./question.ts";
import QuestionCard from "./QuestionCard.vue";
import DeferMenu from "./DeferMenu.vue";
import InspireBox from "./InspireBox.vue";

const props = defineProps<{ workId: number | null }>();

const board = ref<QuestionBoard | null>(null);
/** 正在问的那一张（点开之后）：它已经离开候选池，面板留着它把处置做完 */
const active = ref<SelectedQuestion | null>(null);
const busy = ref(false);
const errorCode = ref("");
const deferFor = ref<number | null>(null);
const inspireFor = ref<number | null>(null);
const justSaved = ref("");

let timer: number | undefined;

/** 报错只给码：句子在字典里（界面文案只有一处来源，别在这里另拼中文）。 */
function report(error: unknown) {
  errorCode.value = asError(error).code;
}

/** 一次完整的刷新：先把到条件的延后放回来，再把新草稿落成卡，最后取最新面板。 */
async function refresh() {
  const work = props.workId;
  if (!work) {
    board.value = null;
    return;
  }
  try {
    busy.value = true;
    errorCode.value = "";
    let next = await questionRequeueDue(work);
    const drafts = await questionDrafts(work);
    if (drafts.length > 0) {
      // 句子由这里渲染（核心零文案），渲染好再交给核心落卡；同模板同锚点不会重复造
      next = await questionSync(
        work,
        drafts.map((draft) => ({
          template_key: draft.template_key,
          body: renderDraft(draft),
          anchors: draft.anchors,
          importance: draft.importance,
        })),
      );
    }
    board.value = next;
    if (active.value && !next.selected.some((item) => item.card_id === active.value?.card_id)) {
      active.value = null; // 处置完就退出"正在问"
    }
  } catch (error) {
    report(error);
  } finally {
    busy.value = false;
  }
}

/** 处置这一类动作的统一收尾：调命令、吃回结果、清掉临时面板。 */
async function dispose(action: () => Promise<QuestionBoard>) {
  try {
    busy.value = true;
    errorCode.value = "";
    board.value = await action();
    active.value = null;
    deferFor.value = null;
    inspireFor.value = null;
  } catch (error) {
    report(error);
  } finally {
    busy.value = false;
  }
}

async function ask(card: SelectedQuestion) {
  try {
    busy.value = true;
    errorCode.value = "";
    board.value = await questionAsk(card.card_id);
    active.value = card;
  } catch (error) {
    report(error);
  } finally {
    busy.value = false;
  }
}

async function saveInspiration(body: string) {
  const cardId = inspireFor.value;
  if (!cardId || !body.trim()) return;
  try {
    busy.value = true;
    errorCode.value = "";
    // 口述那条路落地后，这里的 source 换成 voice / mixed
    const idea = await questionInspire(cardId, body, "typed");
    justSaved.value = t("flow.inspire.saved", { body: idea.body });
    inspireFor.value = null;
    // **状态不动**：只把面板刷新一遍，正在问的那一张还在
    if (props.workId) board.value = await questionBoard(props.workId);
  } catch (error) {
    report(error);
  } finally {
    busy.value = false;
  }
}

const selected = computed(() => board.value?.selected ?? []);
const cooled = computed(() => board.value?.cooled ?? []);
const waiting = computed(() => board.value?.open_deferrals ?? []);
const mutedSources = computed(() => board.value?.muted_sources ?? []);
const mutedClasses = computed(() => board.value?.muted_classes ?? []);

onMounted(() => {
  void refresh();
  // 低频定时：条件可能在这期间满足（写完了那一章、到了那一天）
  timer = window.setInterval(() => void refresh(), 5 * 60_000);
});
onUnmounted(() => {
  if (timer !== undefined) window.clearInterval(timer);
});
watch(() => props.workId, () => void refresh());
</script>

<template>
  <aside class="pane">
    <h2 class="pane__title">{{ t("flow.title") }}</h2>
    <p class="pane__rule">{{ t("flow.rule") }}</p>

    <p v-if="errorCode" class="pane__error">{{ t("flow.error", { code: errorCode }) }}</p>
    <p v-if="busy" class="pane__hint">{{ t("flow.loading") }}</p>

    <!-- 正在问的那一张：处置都在这儿做 -->
    <section v-if="active" class="asking">
      <h3 class="sec">{{ t("flow.asking") }}</h3>
      <p class="asking__body">{{ active.body }}</p>
      <div class="row">
        <button class="act" :disabled="busy" @click="dispose(() => questionPraise(active!.card_id))">
          {{ t("flow.action.praise") }}
        </button>
        <button class="act" :disabled="busy" @click="deferFor = active!.card_id">
          {{ t("flow.action.defer") }}
        </button>
        <button class="act" :disabled="busy" @click="dispose(() => questionDiscard(active!.card_id))">
          {{ t("flow.action.discard") }}
        </button>
        <button class="act" :disabled="busy" @click="dispose(() => questionMuteClass(active!.card_id))">
          {{ t("flow.action.mute_class") }}
        </button>
        <button class="act" :disabled="busy" @click="inspireFor = active!.card_id">
          {{ t("flow.action.inspire") }}
        </button>
      </div>
      <p v-if="justSaved" class="saved">{{ justSaved }}</p>
    </section>

    <DeferMenu
      v-if="deferFor !== null"
      :busy="busy"
      @confirm="(preset, note) => dispose(() => questionDefer(deferFor!, preset, note))"
      @cancel="deferFor = null"
    />

    <InspireBox
      v-if="inspireFor !== null"
      :busy="busy"
      @save="saveInspiration"
      @cancel="inspireFor = null"
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
        @inspire="(cardId) => (inspireFor = cardId)"
      />
    </section>
    <p v-else-if="!busy && board" class="pane__hint">{{ t("flow.empty") }}</p>

    <!-- 在等条件的 -->
    <section v-if="waiting.length > 0">
      <h3 class="sec">{{ t("flow.waiting") }}</h3>
      <p v-for="item in waiting" :key="item.id" class="waiting">
        <span v-if="item.kind === 'time'">{{ dueLabel(item.due_at_ms, Date.now()) }}</span>
        <span v-else-if="item.kind === 'written'">{{ t("flow.waiting.written") }}</span>
        <span v-else>{{ t("flow.waiting.manual") }}</span>
        <em v-if="item.note"> · {{ t("flow.waiting.note", { note: item.note }) }}</em>
        <button class="link" :disabled="busy" @click="dispose(() => questionUndefer(item.card_id))">
          {{ t("flow.waiting.cancel") }}
        </button>
      </p>
    </section>

    <!-- 冷却库：舍弃不等于删除 -->
    <section v-if="cooled.length > 0">
      <h3 class="sec">{{ t("flow.cooled") }}</h3>
      <p class="pane__hint">{{ t("flow.cooled.hint") }}</p>
      <article v-for="item in cooled" :key="item.card_id" class="card">
        <p class="card__dim">{{ item.body }}</p>
        <div class="row">
          <span class="src">{{ sourceLabel(item.source) }}</span>
          <button class="link" :disabled="busy" @click="dispose(() => questionRetrieve(item.card_id))">
            {{ t("flow.action.retrieve") }}
          </button>
          <button
            class="link"
            :disabled="busy"
            @click="dispose(() => questionMuteSource(props.workId as number, item.source))"
          >
            {{ t("flow.source.mute") }}
          </button>
        </div>
      </article>
    </section>

    <!-- 已静音的类别：「这类别再问」的回头路 -->
    <section v-if="mutedClasses.length > 0">
      <h3 class="sec">{{ t("flow.muted_classes") }}</h3>
      <p class="pane__hint">{{ t("flow.muted_classes.hint") }}</p>
      <div class="row">
        <span v-for="key in mutedClasses" :key="key" class="chip">
          {{ classLabel(key) }}
          <button
            class="link"
            :disabled="busy"
            @click="dispose(() => questionUnmuteClass(props.workId as number, key))"
          >
            ✕
          </button>
        </span>
      </div>
    </section>

    <!-- 已静音的来源：能一键让它闭嘴，也能解除 -->
    <section v-if="mutedSources.length > 0">
      <h3 class="sec">{{ t("flow.muted_sources") }}</h3>
      <div class="row">
        <span v-for="source in mutedSources" :key="source" class="chip">
          {{ sourceLabel(source) }}
          <button
            class="link"
            :disabled="busy"
            @click="dispose(() => questionUnmuteSource(props.workId as number, source))"
          >
            ✕
          </button>
        </span>
      </div>
    </section>
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
.sec {
  margin: 12px 0 6px;
  font-size: 11px;
  opacity: 0.75;
}
.card {
  padding: 8px;
  margin-bottom: 6px;
  border: 1px solid var(--ym-line);
  border-radius: 6px;
  background: var(--ym-paper);
}
.card__dim {
  margin: 0 0 6px;
  line-height: 1.6;
  opacity: 0.65;
}
.row {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
  font-size: 11px;
  opacity: 0.85;
}
.act,
.link {
  padding: 2px 8px;
  font-size: 11px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: transparent;
  cursor: pointer;
}
.act:disabled,
.link:disabled {
  opacity: 0.5;
  cursor: default;
}
.asking__body {
  margin: 0 0 8px;
  line-height: 1.6;
}
.waiting {
  margin: 0 0 4px;
  font-size: 12px;
}
.waiting em {
  font-style: normal;
  opacity: 0.7;
}
.chip {
  display: inline-flex;
  gap: 4px;
  align-items: center;
  padding: 1px 6px;
  font-size: 11px;
  border: 1px solid var(--ym-line);
  border-radius: 10px;
}
.saved {
  margin: 6px 0 0;
  font-size: 11px;
  opacity: 0.8;
}
</style>
