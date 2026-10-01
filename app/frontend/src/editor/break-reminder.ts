// 用眼提醒：**只在"作者要停下来"的时刻说一句**——这是本项目那条"安静出口"
// 长出来的第一块内容：非阻塞、一次一句、说完就走，绝不打断写作。
//
// 四条纪律（任务口径，一条都不许松）：
// - **只在停下点评估**：切章 / 切书 / 关窗前那一下（见 `useBreakReminder` 接的三处）。
//   击键那条快路上一句都不评估——写作中绝不开口；
// - **同一轮不重复**：说过就记账；静置够久（`IDLE_RESET_MS`，5 分钟没有活动）才算翻篇、
//   下一轮重新算；
// - **关掉 = 一声不响**：不评估、不显示、不记任何东西（连"提醒过了"都不记）；
// - **不指挥作者**：条上只有一句话 + 一个「知道了」，不给"去休息"这种指令性按钮。
//
// 判据两条，**各自记账、互不覆盖**：
// - **连着写满 N 分钟**：只在"作者还在动"时累加；活动信号与叩问那条"卡住"同源——
//   会话里 `saveState.char_count` 一变（落盘回执）就是还在写。静置够久 ⇒ 计数清零；
// - **今天写到 M 字**：读的是**今日累计**（与状态栏同一个读数、同一套口径），不是这一章
//   本次写了多少。它的账是**一整天**的：阈值问的本来就是"今天"，所以静置不翻篇它，
//   只有今日字数掉回阈值以下（换了一天，或者作者删了字）才重新算。
//
// 本文件是**纯逻辑 + 注入时钟**：它不认识 Vue、不认识 DOM，单测直接喂时钟与读数。
// 下面那半 `useBreakReminder` 才是接线（与会话的三个停下点接上），薄薄一层。

// ★ 那条出口上还住着**第二个内容**：**章长提醒（防爆章）**
//   （"这一章 4700 字，这本书中位 2000 字（2.4 倍）——是故意的吗？"）。
//   它跟用眼提醒**同一处停下点、同一条小条**，只是它要读一眼书里的分布（异步）；
//   判据与记账都在 `editor/chapter-length.ts`；答复多一个「就这样写」（这一章以后不再提）。
//   优先级：叩问推 > 备份 > **用眼提醒 > 章长**——用眼那两条一轮只说一次，先让它们说完；
//   章长这条每次停下点都可能再提（「知道了」只关这一次），排最后就不会把前两条饿死。
import { ref, watch, type Ref } from "vue";

import { t } from "../locales/index.ts";
import type { EditorSession } from "./session.ts";
import { pickDayCount } from "./writing-days.ts";

import {
  chapterTipText,
  useChapterLengthWatch,
  type ChapterLengthTransport,
  type ChapterLengthTip,
} from "./chapter-length.ts";

/** 静置多久算"翻篇"（毫秒）：连续写作的计数清零、下一个停下点重新算。 */
export const IDLE_RESET_MS = 5 * 60_000;

/** 两个阈值在**界面**这一侧的可接受范围（与核心同口径；核心才是真正的闸门）。 */
export const BREAK_MINUTES_RANGE = { min: 1, max: 180 } as const;
export const BREAK_WORDS_RANGE = { min: 1, max: 20_000 } as const;

/** 现在生效的那份偏好（核心读回来的；读不出来给 `null`）。 */
export interface BreakSettings {
  /** 开关；关着就一声不响 */
  on: boolean;
  /** 连着写满多少分钟说一句 */
  minutes: number;
  /** 今天写到多少字说一句 */
  words: number;
}

/** 条上要显示的那一句是什么（`null` = 不显示）。 */
export type BreakTip =
  | { kind: "minutes"; minutes: number }
  | { kind: "words"; words: number }
  /** 章长那条：事实 + 那条"以后不再提"的记号的原文（核心给的，界面照抄） */
  | ({ kind: "chapter" } & ChapterLengthTip);

export interface BreakWatchOptions {
  /**
   * 现在生效的偏好；**读不出来给 `null`**——那就不响（与"读不回来不抢焦点"同一条安全边：
   * 宁可少说一句，也不要拿一个猜出来的阈值去开口）。
   */
  settings: () => BreakSettings | null;
  /** 当前这一章的字数（落盘回执）——它一变就是"作者还在动" */
  chars: () => number;
  /** 今天累计写了多少（状态栏那个读数；口径由调用方定） */
  todayWords: () => number;
  /** 现在几点（注入：单测喂假时钟） */
  now?: () => number;
}

export interface BreakWatch {
  /** 条上要显示的那一句（null = 不显示） */
  tip: Ref<BreakTip | null>;
  /** 观察到一次活动（调用方在字数变化时喂一口）：只在有活动时累加"还在动"的时间 */
  poke: () => void;
  /** 到了一个停下点：评估一次（该说就说；关着 / 说过了 / 静置太久都什么都不做） */
  evaluate: () => void;
  /** 「知道了」：把条收掉（收掉不等于没说过——这一轮仍然不重复） */
  dismiss: () => void;
  /**
   * 章长那条的入口：**只在停下点、读完那一页之后调**（接线见 `useBreakReminder` 那三处）。
   * 条上已经有一条时什么都不做——**软件不该抢自己的话头**。
   */
  offerChapter: (tip: ChapterLengthTip | null) => void;
  /** ★**总开关**：关掉那个开关＝**全部安静提醒**闭嘴（用眼两条在 `evaluate` 里拦，这条在出口拦）。 */
  enabled: () => boolean;
}

export function useBreakWatch(options: BreakWatchOptions): BreakWatch {
  const now = options.now ?? Date.now;
  const tip = ref<BreakTip | null>(null);

  /** 上一次"作者还在动"的时刻 */
  let lastActiveAt = now();
  /** 上一眼看到的字数（第一次观察只记数，不当成一次活动） */
  let lastChars = options.chars();
  /** 这一轮累计"还在动"了多久（只在有活动时累加） */
  let activeMillis = 0;
  /** 这一轮"连着写满"那条说过了没有（静置翻篇时清掉） */
  let saidMinutes = false;
  /** "今天写到"那条说过了没有（今日字数掉回阈值以下时清掉——换天或删了字） */
  let saidWords = false;

  /** 静置够久：翻篇——连着写的计数清零、下一轮重新算。 */
  function turnOver(at: number): void {
    activeMillis = 0;
    saidMinutes = false;
    lastActiveAt = at;
  }

  function poke(): void {
    const chars = options.chars();
    if (chars === lastChars) return; // 没动就不算活动
    lastChars = chars;
    const at = now();
    const gap = at - lastActiveAt;
    // 静置够久 ⇒ 上一轮已经翻篇了：从这里重新起算（不是接着上一轮累加）
    if (gap >= IDLE_RESET_MS) {
      turnOver(at);
      return;
    }
    activeMillis += gap;
    lastActiveAt = at;
  }

  function evaluate(): void {
    const settings = options.settings();
    // 关掉 / 读不回来：一声不响——不评估、不显示、也不记账
    if (settings === null || !settings.on) return;
    if (tip.value !== null) return; // 条上已经有一条：软件不该抢自己的话头
    const at = now();
    // 到停下点时已经静置够久：这一轮算过去了（计数清零、下一个停下点重新算）
    if (at - lastActiveAt >= IDLE_RESET_MS) turnOver(at);

    // 第一条：连着写满 N 分钟（阈值非正数＝这一条关掉）
    if (!saidMinutes && settings.minutes > 0 && activeMillis >= settings.minutes * 60_000) {
      saidMinutes = true;
      tip.value = { kind: "minutes", minutes: Math.round(activeMillis / 60_000) };
      return;
    }

    // 第二条：今天写到 M 字（**按今日累计判**，不是按这一章本次写了多少）
    const today = options.todayWords();
    if (settings.words <= 0 || today < settings.words) {
      saidWords = false; // 还没到 / 掉回阈值以下（换天、删了字）：重新算
      return;
    }
    if (saidWords) return;
    saidWords = true;
    tip.value = { kind: "words", words: today };
  }

  /**
   * 章长那条：**只在停下点调**（异步读完那一页之后）——条空着才说得出来。
   *
   * 它与用眼那两条**各自记账**：用眼那两条一轮只说一次，章长这条每次停下点都可能再提
   * （「知道了」＝只关这一次，不记账）；「就这样写」才是记账那一种（落在 `chapter-length.ts`）。
   */
  /** 这一处安静提醒现在开没开（＝那份设置里的总开关；界面与章长那条都问它）。 */
  const enabled = (): boolean => {
    const current = options.settings();
    return current !== null && current.on;
  };

  const offerChapter = (next: ChapterLengthTip | null): void => {
    if (!enabled()) return; // 总开关关着：章长这条也闭嘴（作者要的是安静，不是"少吵一条"）
    if (next === null || tip.value !== null) return;
    tip.value = { kind: "chapter", ...next };
  };

  const dismiss = (): void => {
    tip.value = null;
  };

  return { tip, poke, evaluate, dismiss, offerChapter, enabled };
}

/** 条上那一句（键名只在这里出现一次；组件不必知道有哪几种说法）。 */
export function breakTipText(tip: BreakTip | null): string {
  if (tip === null) return "";
  if (tip.kind === "minutes") return t("break.eye", { minutes: tip.minutes });
  if (tip.kind === "words") return t("break.words", { words: tip.words });
  return chapterTipText(tip);
}

export interface BreakReminder {
  tip: Ref<BreakTip | null>;
  /** 「知道了」：只关这一次（不记账——下次停下点还超长会再提一次） */
  dismiss: () => void;
  /** 「就这样写」：这一章以后不再提（只在章长那条上有意义） */
  keep: () => void;
}

/**
 * 接到会话上：**三个停下点**各接一次，另外把活动信号喂给纯逻辑。
 *
 * - 切章（目录树换节点）、切书（换作品）：开机第一次落定不算停下点——那一刻作者刚打开
 *   软件，"停下来"三个字还谈不上；
 * - 关窗前那一下：走的是既有的退出那条路（`session.exitState.blocked` 被点亮＝保存没落干净、
 *   退出对话框那一层亮出来了）。**如实说一句**：这一处的提醒压在遮罩底下，而且退出这条路
 *   正常情况下就走成了（重试 / 导出 / 仍然退出三条都通向关窗）——所以它多半只是"评估了、
 *   记账了"，作者多半看不到；真要在他合上稿子之前说一句，得有一条**不阻塞**的关窗路
 *   （那是另一件事，不在这一刀里）。另外两处（切章 / 切书）才是这句话真正露面的时候。
 */
export function useBreakReminder(
  session: EditorSession,
  chapterTransport: ChapterLengthTransport,
): BreakReminder {
  const watch0 = useBreakWatch({
    settings: () => {
      const values = session.appearance.values.value;
      // 读不回来（还没读到 / 读失败）：不响。默认值只在核心那一处，界面不猜
      if (values === null) return null;
      return {
        on: values.break_reminder,
        minutes: values.break_minutes,
        words: values.break_words,
      };
    },
    chars: () => session.saveState.value.char_count,
    // 今日：与状态栏那个读数同一套读法（口径由核心落定，界面不自己排对照表）
    todayWords: () => pickDayCount(session.writing.today.value, session.caliber.value),
  });

  // 章长那条：与用眼提醒**同一处停下点**（不新起钩子），但它要读一眼书里的分布，所以是
  // 异步的——读完把结论交给同一个出口（`offerChapter`，条空着才说）。
  const chapter = useChapterLengthWatch(chapterTransport);
  /**
   * 刚离开的那一章（连同**它在哪本书里**）：切书那一刻手上这一对还是上一本书的，
   * 而判据要拿它跟**它自己那本书**的分布比——所以两个值一起记。
   */
  let leftWork = session.workId.value;
  let leftChapter = session.directory.current.value;
  /**
   * 读一眼、判"刚离开的那一章"、把结论交给出口。
   *
   * 默认用上一对（`leftWork` / `leftChapter`）；关窗前那一处显式传"现在手上这一对"
   * （那一刻这一章就是要离开的那一章）。读不出来 / 这一章不偏长：一声不响。
   */
  function stopChapterLength(
    work: number | null = leftWork,
    node: number | null = leftChapter,
  ): void {
    // ★**一个开关静音全部安静提醒**（用户拍板）：关掉那个开关之后，章长这条也闭嘴——
    // 不去读分布、也不摆条。理由：作者说"别提醒我"时，他要的是安静，不是"少吵一条"。
    // 用眼那两条的闸门在 `useBreakWatch` 里（同一个 settings）；这一条在接线层拦，
    // 两处读的是**同一个** `break_reminder`。
    if (!watch0.enabled()) return;
    void chapter.atStop(work, node).then((next) => watch0.offerChapter(next));
  }

  // 活动信号：字数一变（落盘回执）就是"还在写"——与叩问那条"卡住"用的是同一个观测点
  watch(() => session.saveState.value.char_count, () => watch0.poke());

  // 停下点一：切章（**先喂一口活动**再评估——换章本身会改字数，顺序摆在这里免得看运气）
  let hadChapter = session.directory.current.value !== null;
  watch(
    () => session.directory.current.value,
    (node) => {
      if (node === null) return;
      watch0.poke();
      if (hadChapter) {
        watch0.evaluate(); // 用眼提醒（同步）
        stopChapterLength(); // 章长：读一眼**刚离开的那一章**
      }
      hadChapter = true;
      leftChapter = node;
      leftWork = session.workId.value;
    },
  );

  // 停下点二：切书（从一个作品换到另一个才算；开机第一次定下当前书不算）
  let hadWork = session.workId.value !== null;
  watch(
    () => session.workId.value,
    (id) => {
      if (hadWork && id !== null) watch0.evaluate();
      hadWork = id !== null;
      // 章长那一半**不在这里重复评一次**：换书是先换章再换书，刚离开的那一章由上面那个
      // 切章 watcher 带上了（这里再评只会拿新书那份分布去比一个刚打开的章，没有意义）。
      // 这一处只把**新书的分布**读进来给目录角标（不发声："刚打开这本书"不是一个要说话的时刻）。
      void chapter.refresh(id);
    },
  );

  // 停下点三：关窗前那一下（退出那条路走起来、退出对话框亮出来那一刻）
  watch(
    () => session.exitState.value.blocked,
    (blocked) => {
      if (!blocked) return;
      watch0.evaluate();
      // 章长：这一刻手上这一章就是要离开的那一章。⚠️ **与用眼提醒同一处弱环**——这一句
      // 压在退出确认框那层遮罩底下，作者多半看不到（如实说，不去改退出那条链）。
      stopChapterLength(session.workId.value, session.directory.current.value);
    },
  );

  const keep = (): void => {
    const current = watch0.tip.value;
    if (current === null || current.kind !== "chapter") return;
    // 先把条收掉（说完就走），再落账：写失败也**不**假装记上了——下一回读库还是"没说过"，
    // 这一章会再被提一次（那是实话）。
    watch0.dismiss();
    void chapter.keep(current.work_id, current.node_id, current.fingerprint);
  };

  return { tip: watch0.tip, dismiss: watch0.dismiss, keep };
}
