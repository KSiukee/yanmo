// 正文检索：打几个字 → 停手一会儿（或按回车）→ 找一趟 → 列出来 → 点一条跳过去。
//
// 三条分寸：
// 1. **过时的响应不许盖新的**：每发一趟记一个序号（与 autosave 的 writeEpoch 同一手法），
//    旧那一笔回来时认不出自己，直接丢掉——否则"先打三个字、再打五个字"时，慢的那一趟
//    会把新结果顶掉，作者看到的是上一个问题的答案；
// 2. **没问的别发车**：trim 之后是空的就只清结果，一趟都不发（空问题在核心那边是白扫全库）；
// 3. **句子不是这里的事**：这里只留核心/传输层给的详情，措辞由界面查字典渲染，
//    状态机里一个中文都不出现（界面文案只有 `locales/` 一处来源）。
//
// 计时器与传输都是注入的（与 autosave 同一个范式）：测试拿假计时器把防抖窗口拨过去，
// 不必真的等 250ms。

import { ref, type Ref } from "vue";

import type { SearchHit } from "../api/core";

/** 一趟检索。形状与网关的 `searchText` 一致——会话层把这个函数原样递进来。 */
export type SearchTransport = (
  query: string,
  work_id: number | null,
  limit: number,
) => Promise<SearchHit[]>;

/** 一次最多取几条：够到它就如实说"只显示前 N 条"。 */
export const SEARCH_LIMIT = 50;
/** 停手多久才发车——与自动落盘同一个量级：太快白扫全库，太慢觉得钝。 */
const DEFAULT_DEBOUNCE_MS = 250;

type TimerHandle = ReturnType<typeof setTimeout>;

export interface SearchOptions {
  transport: SearchTransport;
  /** 点一条结果：跳到那一章（"先落盘再切章"那条纪律在会话层） */
  openHit: (hit: SearchHit) => Promise<void>;
  /** 出错的详情往哪儿说（会话层并进状态栏那句交代）；**不产句子** */
  report?: (detail: string) => void;
  schedule?: (fn: () => void, ms: number) => TimerHandle;
  cancel?: (handle: TimerHandle) => void;
  debounceMs?: number;
}

export interface Search {
  visible: Ref<boolean>;
  /** 输入框里那一串（作者写的原文，没 trim） */
  query: Ref<string>;
  hits: Ref<SearchHit[]>;
  /** 正在找（界面据此说一句"正在找"） */
  busy: Ref<boolean>;
  /** 出错的详情；空串＝没出错。**句子由界面查字典渲染** */
  error: Ref<string>;
  /** 命中数够到了上限：界面要如实说"只显示前 N 条" */
  truncated: Ref<boolean>;
  /** 顶栏那个按钮：开着就收、收着就开 */
  toggle: () => void;
  close: () => void;
  /** 输入框动一下：排一次防抖（到点才发车） */
  type: (text: string) => void;
  /** 立刻找（回车用，不等防抖） */
  run: () => Promise<void>;
  /** 点一条结果：跳过去并把弹窗收起来（**只叫一次**） */
  open: (hit: SearchHit) => Promise<void>;
}

export function useSearch(options: SearchOptions): Search {
  const visible = ref(false);
  const query = ref("");
  const hits = ref<SearchHit[]>([]);
  const busy = ref(false);
  const error = ref("");
  const truncated = ref(false);

  const schedule = options.schedule ?? ((fn: () => void, ms: number) => setTimeout(fn, ms));
  const cancel = options.cancel ?? ((handle: TimerHandle) => clearTimeout(handle));
  const debounceMs = options.debounceMs ?? DEFAULT_DEBOUNCE_MS;

  /** 排着的那一次防抖：新的顶掉旧的，到点发的是最后那串 */
  let timer: TimerHandle | null = null;
  /** 作废计数器：被作废的那一趟回来时认不出自己，不许再动结果（见文件头第 1 条） */
  let epoch = 0;
  /** 正在跳（连点两下只跳一次） */
  let opening = false;

  function cancelTimer(): void {
    if (timer === null) return;
    cancel(timer);
    timer = null;
  }

  /** 把上一次的答案清干净（空问题、以及"这一趟已经不作数了"时用）。 */
  function clear(): void {
    hits.value = [];
    truncated.value = false;
    error.value = "";
  }

  function detailOf(cause: unknown): string {
    return cause instanceof Error ? cause.message : String(cause);
  }

  async function run(): Promise<void> {
    cancelTimer();
    const asked = query.value.trim();
    if (!asked) {
      // 空问题：不发车，并把在飞的那一趟作废（否则它回来时会把结果摆回来）
      epoch += 1;
      busy.value = false;
      clear();
      return;
    }
    const mine = (epoch += 1);
    busy.value = true;
    error.value = "";
    try {
      const found = await options.transport(asked, null, SEARCH_LIMIT);
      if (mine !== epoch) return; // 过时的那一趟：不许盖新的
      hits.value = found;
      truncated.value = found.length >= SEARCH_LIMIT;
    } catch (cause) {
      if (mine !== epoch) return;
      hits.value = [];
      truncated.value = false;
      error.value = detailOf(cause);
      options.report?.(error.value);
    } finally {
      // 只有"当下这一趟"才有权改忙标记：新那一趟还排着，别提前说找完了
      if (mine === epoch) busy.value = false;
    }
  }

  function toggle(): void {
    visible.value = !visible.value;
  }

  function close(): void {
    cancelTimer(); // 关掉了就别再自己发车（结果留着：回头打开还在）
    visible.value = false;
  }

  return {
    visible,
    query,
    hits,
    busy,
    error,
    truncated,
    toggle,
    close,
    type: (text) => {
      query.value = text;
      cancelTimer();
      if (!text.trim()) {
        epoch += 1; // 空输入之后，在飞的那一趟也不许再往结果里写
        busy.value = false;
        clear();
        return;
      }
      timer = schedule(() => {
        timer = null;
        void run();
      }, debounceMs);
    },
    run,
    open: async (hit) => {
      if (opening) return;
      opening = true;
      try {
        await options.openHit(hit);
      } catch {
        // 跳不过去：切章/切书那边已经把原因报出来了（会话层），这里不重复喊
      } finally {
        opening = false;
        close();
      }
    },
  };
}

/** 片段里的一段：`hit` 为真就是要标出来的那一处。 */
export interface HighlightSegment {
  text: string;
  hit: boolean;
}

/**
 * 把片段切成"要标出来的那一段 + 其余两段"。
 *
 * 三条分寸：
 * - 大小写不敏感（英文稿子里问 `the`，`The` 也得标到）；
 * - **只标第一处**——同一串在一句话里出现好几次时，整段标满只是噪音；
 * - 查不到 / 问题为空：整段原样一段（`hit: false`）。
 *
 * 硬不变量：**每一段拼回去必须与原文逐字相同**——所以切的一律是**原文**的下标；
 * 小写那份只拿来比位置，绝不拿它当输出（有的字符大小写转换会改变长度，
 * 拿小写串的下标去切原文就会错位）。
 */
export function highlight(snippet: string, query: string): HighlightSegment[] {
  const whole: HighlightSegment[] = [{ text: snippet, hit: false }];
  const needle = query.trim();
  if (!needle || snippet.length < needle.length) return whole;
  const lower = needle.toLowerCase();
  let at = -1;
  for (let index = 0; index + needle.length <= snippet.length; index += 1) {
    // 逐位比**同一长度**的小写窗口：命中的位置自然落在原文的下标上
    if (snippet.slice(index, index + needle.length).toLowerCase() === lower) {
      at = index;
      break;
    }
  }
  if (at < 0) return whole;
  const parts: HighlightSegment[] = [];
  if (at > 0) parts.push({ text: snippet.slice(0, at), hit: false });
  parts.push({ text: snippet.slice(at, at + needle.length), hit: true });
  if (at + needle.length < snippet.length) {
    parts.push({ text: snippet.slice(at + needle.length), hit: false });
  }
  return parts;
}
