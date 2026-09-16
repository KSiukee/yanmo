// 主动问一句（推）：**什么时候开口、开口之后那条东西怎么收**。
//
// 门槛在核心（日配额 + 冷却 + 挑哪张卡 + 算"已问"）；这里只管"什么时候值得问一次"——
// 三个时机，任务里定死的：
//
// 1. **开新章**：作者到了一章还没写字的地方（正要开头）；
// 2. **卡住**：连续一段时间没有击键，而且这一章已经写了点东西（写到一半停住了）；
// 3. **写完一章**：切走时刚离开的那一章有正文（刚收工，适合回头看一眼）。
//
// 三条纪律：
// - **一次只冒一条**：提示条上正显示着就不再问（软件不该抢自己的话头）；
// - **不主动打扰设置面板**：作者在设置里把次数设成 0 就一次都不问（核心那边也会拦）；
// - **错过一次推是静默的**：没得问、配额用完、冷却没到，都不报错——那本来就不该让人知道。
//
// 判据全从**会话现成的状态**里读（当前章 + 字数），不额外塞钩子进编辑器：这样它与
// "打字/落盘"那条快路完全解耦，读的也只是"作者还在不在动"这一个事实。
import { onUnmounted, ref, watch, type Ref } from "vue";

import { asError } from "../api/errors.ts";
import { questionPush, type PushDone, type SelectedQuestion } from "../api/question.ts";
import type { EditorSession } from "../editor/session.ts";
import { localDay } from "./display.ts";

/** 为什么现在开口（进留痕的 trigger，事后看得出是哪条时机触发的）。 */
export type PushReason = "new_chapter" | "idle" | "chapter_done";

/** 卡住多久算卡住（分钟）——固定判据，不进设置（要调再给它开一栏）。 */
export const IDLE_MINUTES = 15;

/** 每次检查间隔（毫秒）。 */
const TICK_MS = 60_000;

export interface QuestionPushOptions {
  session: EditorSession;
  /** 当前生效的打扰度（每天几次 / 冷却多少分钟）——来自作者偏好 */
  quota: () => { perDay: number; cooldownMinutes: number };
  /** 问核心要一次推（注入真命令，测试能给替身） */
  ask?: (
    workId: number,
    nodeId: number | null,
    perDay: number,
    cooldownMinutes: number,
    today: number,
    reason: PushReason,
  ) => Promise<PushDone>;
  onError?: (code: string) => void;
}

export interface QuestionPush {
  /** 提示条上要显示的那一句（null = 不显示） */
  tip: Ref<SelectedQuestion | null>;
  /** 「先不用」：把条收掉（这一条已经算问过了，收掉不代表没问） */
  dismiss: () => void;
  /** 「答一句」：把这张卡交给面板打开（面板那边接住），并收掉条 */
  take: () => SelectedQuestion | null;
  /** 作者本地那一天（`YYYYMMDD`）——配额按本地那一天算 */
  today: () => number;
}

export function useQuestionPush(options: QuestionPushOptions): QuestionPush {
  const tip = ref<SelectedQuestion | null>(null);
  const ask = options.ask ?? questionPush;

  /** 上次"作者还在动"的时刻（毫秒）——卡住判据认它 */
  let lastActivityAt = Date.now();
  /** 上一次看到的（哪一章、写了多少字）：切章时靠它判断"刚离开的那一章有没有内容" */
  let lastSeen: { node: number | null; chars: number } = { node: null, chars: 0 };
  /** 第一次观察不算时机（刚打开软件/刚开一章，不该立刻冒头） */
  let ready = false;

  /** 问一次；成了就把条摆出来。没成（配额/冷却/没得问）**什么都不做**。 */
  async function consider(reason: PushReason) {
    if (tip.value !== null) return;
    const work = options.session.workId.value;
    if (work === null) return;
    const { perDay, cooldownMinutes } = options.quota();
    if (perDay <= 0) return; // 作者说了别打扰——连问都不必问
    try {
      const done = await ask(
        work,
        options.session.directory.current.value,
        perDay,
        cooldownMinutes,
        localDay(),
        reason,
      );
      if (done.question) tip.value = done.question;
    } catch (error) {
      // 错过的推不打扰作者；只把码交给上层（诊断用）
      options.onError?.(asError(error).code);
    }
  }

  const dismiss = () => {
    tip.value = null;
  };
  const take = () => {
    const card = tip.value;
    tip.value = null;
    return card;
  };

  // 作者的动静：字数一变（落盘回执）就刷新"还在动"的时刻
  watch(
    () => options.session.saveState.value.char_count,
    () => {
      lastActivityAt = Date.now();
    },
  );

  // 切章：走了的有没有内容 / 新到的这一章是不是空的
  watch(
    () => [options.session.directory.current.value, options.session.saveState.value.char_count] as const,
    ([node, chars]) => {
      const left = lastSeen;
      lastSeen = { node, chars };
      if (!ready) {
        ready = true;
        lastActivityAt = Date.now();
        return;
      }
      if (left.node === node) return; // 同一章里字数变了，不算"切章"
      if (left.node !== null && left.chars > 0) {
        void consider("chapter_done"); // 刚写完一章就走
      } else if (chars === 0) {
        void consider("new_chapter"); // 到了一章还没写字的地方
      }
      lastActivityAt = Date.now();
    },
    { immediate: true },
  );

  const timer = window.setInterval(() => {
    const state = options.session.saveState.value;
    if (state.char_count <= 0) return; // 空章不算"卡住"，那是还没开始
    if (Date.now() - lastActivityAt < IDLE_MINUTES * 60_000) return;
    lastActivityAt = Date.now(); // 冒过一次就重新计时（不然每分钟都来）
    void consider("idle");
  }, TICK_MS);
  onUnmounted(() => window.clearInterval(timer));

  return { tip, dismiss, take, today: () => localDay() };
}
