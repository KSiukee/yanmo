// 排版清理（dry-run）：**先看后改**——扫出建议、逐条勾选、只改勾中的那几处。
//
// 三条分寸：
// - 扫描与应用都在核心（纯函数、可单测），这里只转发与管状态；
// - **绝不默认全改**：默认只勾"建议改"那一档（省略号 / 破折号 / 段首空白），
//   会动到书写习惯的（衍字、半角标点、连着打的标点、引号）与纯风格的（中英之间加空格）
//   一律默认不勾，由作者自己选；核心说"没有确定改法"的（引号缺一半）只列在提醒里，不可勾；
// - 应用前先给这一章留个版本；稿子在预览之后又变过就重新扫一遍——**不拿旧序号去改新稿子**
//   （核心那头也会整批拒绝，这里先拦住，好让作者看见原因）。
//
// 只认接口不认具体命令（真命令在会话层注入）：这套状态机可以脱离界面与核心单测。

import { ref, type Ref } from "vue";

import type { TypesetChange, TypesetNotice, TypesetOptions, TypesetReport, TypesetRule } from "../api/core";
import { t } from "../locales/index.ts";

/** 核心的默认引号风格（`curly`）。界面只在偏好还没读回来时用它顶一下，不另立一套默认。 */
export const DEFAULT_QUOTE_STYLE = "curly";

/** 排版清理要用的几个动作（会话层注入真命令，测试注入替身）。 */
export interface TypesetTransport {
  rules: () => Promise<TypesetRule[]>;
  scan: (text: string, options: TypesetOptions) => Promise<TypesetReport>;
  apply: (text: string, options: TypesetOptions, accepted: number[]) => Promise<string>;
}

export interface TypesetOptionsIn {
  transport: TypesetTransport;
  /** 当前这一章的正文（作者手上那份，可能还没落盘） */
  currentText: () => string;
  /** 作者选过的引号风格（从偏好里读） */
  savedQuoteStyle: () => string;
  /** 换了引号风格：落库（写不进去也不拦着这次清理，只是下次不记得） */
  onQuoteStyle: (style: string) => void;
  /** 动手之前给这一章留个版本；失败会抛异常——那就别改 */
  beforeApply: () => Promise<void>;
  /** 正文已换成改好的那一版：会话层据此换编辑器内容并让落盘跟上 */
  onApplied: (text: string) => Promise<void> | void;
  onError?: (message: string) => void;
}

export interface TypesetState {
  visible: Ref<boolean>;
  /** 规则清单（顺序就是界面上的顺序，也是同位置冲突时的优先级） */
  rules: Ref<TypesetRule[]>;
  changes: Ref<TypesetChange[]>;
  /** **只报告不给改法**的提醒（缺一半的引号之类）：列出来，但不动正文 */
  notices: Ref<TypesetNotice[]>;
  /** 勾中的改动下标 */
  picked: Ref<Set<number>>;
  /** 引号用哪一套（作者在面板里改） */
  quoteStyle: Ref<string>;
  /** 上一次动作的交代（"改好了 N 处"） */
  note: Ref<string>;
  busy: Ref<boolean>;
  /** 打开：现扫一遍（**刚打开时正文一个字都不动**） */
  open: () => Promise<void>;
  close: () => void;
  setQuoteStyle: (style: string) => void;
  toggleChange: (index: number) => void;
  /** 整条规则一起勾 / 一起取消 */
  toggleRule: (code: string, on: boolean) => void;
  apply: () => Promise<void>;
}

export function useTypeset(options: TypesetOptionsIn): TypesetState {
  const visible = ref(false);
  const rules = ref<TypesetRule[]>([]);
  const changes = ref<TypesetChange[]>([]);
  const notices = ref<TypesetNotice[]>([]);
  const picked = ref<Set<number>>(new Set());
  const quoteStyle = ref(DEFAULT_QUOTE_STYLE);
  const note = ref("");
  const busy = ref(false);
  /** 上次扫描时那份正文的原文：它变了就说明预览过期了 */
  let scanned = "";

  const report = (error: unknown) =>
    options.onError?.(error instanceof Error ? error.message : String(error));

  const ask = (): TypesetOptions => ({ quote_style: quoteStyle.value });

  /** 默认勾中的：只有"建议改"那一档。 */
  function defaultPicked(): Set<number> {
    const safe = new Set(rules.value.filter((rule) => rule.tier === "safe").map((rule) => rule.code));
    const out = new Set<number>();
    changes.value.forEach((change, index) => {
      if (safe.has(change.rule)) out.add(index);
    });
    return out;
  }

  /** 扫一遍（顺带把规则清单取回来一次）。 */
  async function scan(text: string): Promise<void> {
    if (rules.value.length === 0) rules.value = await options.transport.rules();
    const report = await options.transport.scan(text, ask());
    changes.value = report.changes;
    notices.value = report.notices;
    scanned = text;
    picked.value = defaultPicked();
  }

  return {
    visible,
    rules,
    changes,
    notices,
    picked,
    quoteStyle,
    note,
    busy,
    open: async () => {
      if (busy.value) return;
      busy.value = true;
      note.value = "";
      quoteStyle.value = options.savedQuoteStyle();
      try {
        await scan(options.currentText());
        visible.value = true;
      } catch (error) {
        report(error);
      } finally {
        busy.value = false;
      }
    },
    close: () => {
      visible.value = false;
    },
    setQuoteStyle: (style) => {
      if (quoteStyle.value === style) return;
      quoteStyle.value = style;
      options.onQuoteStyle(style);
      // 换了引号那一套，建议跟着变：重扫一遍（勾选也回到默认那一档）
      void scan(options.currentText()).catch(report);
    },
    toggleChange: (index) => {
      const next = new Set(picked.value);
      if (next.has(index)) next.delete(index);
      else next.add(index);
      picked.value = next;
    },
    toggleRule: (code, on) => {
      const next = new Set(picked.value);
      changes.value.forEach((change, index) => {
        if (change.rule !== code) return;
        if (on) next.add(index);
        else next.delete(index);
      });
      picked.value = next;
    },
    apply: async () => {
      if (busy.value) return;
      const accepted = [...picked.value].sort((left, right) => left - right);
      if (accepted.length === 0) {
        note.value = t("typeset.done_none");
        return;
      }
      const text = options.currentText();
      if (text !== scanned) {
        // 预览之后又打过字：旧的勾选序号已经对不上，重扫一遍让作者重新看
        note.value = t("typeset.stale");
        await scan(text).catch(report);
        return;
      }
      busy.value = true;
      try {
        await options.beforeApply(); // 先留底：留不下就别改
        const fixed = await options.transport.apply(text, ask(), accepted);
        await options.onApplied(fixed);
        note.value = t("typeset.done", { count: accepted.length });
        await scan(fixed); // 改完再扫一遍：剩下的建议接着看
      } catch (error) {
        report(error);
      } finally {
        busy.value = false;
      }
    },
  };
}

/** 一处改动在界面上的样子：勾没勾、在第几段、前后是什么。 */
export interface TypesetItem {
  change: TypesetChange;
  index: number;
  picked: boolean;
}

/** 一条规则连同它名下的改动。 */
export interface TypesetGroup {
  rule: TypesetRule;
  items: TypesetItem[];
  /** 这一条勾了几处 */
  picked: number;
}

/** 按规则分组，只列有命中的；顺序照核心给的规则清单（界面不自己排序）。 */
export function groupChanges(
  rules: TypesetRule[],
  changes: TypesetChange[],
  picked: ReadonlySet<number>,
): TypesetGroup[] {
  return rules
    .map((rule) => {
      const items = changes
        .map((change, index) => ({ change, index, picked: picked.has(index) }))
        .filter((item) => item.change.rule === rule.code);
      return { rule, items, picked: items.filter((item) => item.picked).length };
    })
    .filter((group) => group.items.length > 0);
}
