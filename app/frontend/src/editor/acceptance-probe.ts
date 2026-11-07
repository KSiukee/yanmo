// i18n-allow-file: 本文件里产出的字符串是**验收报告的附注**（写进报告文件的那一行字），
// 不是界面文案；界面文案仍然只在 frontend/src/locales/（与壳侧 acceptance 那个文件同一性质）。
//
// 界面级验收的自测序列：切章 / 输入往返 / 滚动 / 检索。
//
// 为什么放在界面里量：这四件事的真身都在这一侧（先落盘再切的编排、编辑器的更新与落盘、
// 防抖与列表渲染）。壳那边只收数字——**口径由每一条的 note 写清**，别与命令行的口径
// 混进同一个键（命令行的 `search` 量的是核心检索路径，这里量的是"作者按下去到看见"）。
//
// 本文件是**纯编排**（动作全部注入，不 import 任何 api/界面模块）：所以它能在 node 里单测。
// 真接线在 `acceptance-probe-wire.ts`（那一个才 import Tauri 通道）。

/** 一段自测要的"真动作"（注入是为了可单测）。 */
export interface ProbeDeps {
  /** 能切的章（至少两章才量得动；只有一章就如实写"没量到"） */
  chapters: () => number[];
  /** 点目录里的一行（会话层负责"先落盘再切"） */
  openChapter: (nodeId: number) => Promise<void>;
  insertText: (text: string) => void;
  /** 等这一笔落盘完成（超时也要返回，别把验收挂死） */
  waitSaved: () => Promise<void>;
  scroll: () => void;
  /** 打开搜索、输入、等结果列表渲染完 */
  search: (query: string) => Promise<void>;
  /** 等一帧（两次 rAF：先提交再布局） */
  frame: () => Promise<void>;
  now: () => number;
}

/** 交回壳的一条（名字 / 毫秒 / 附注；口径写在 note 里）。 */
export interface ProbeStep {
  name: string;
  ms: number;
  note: string;
}

/** 切几次章（两章来回）。 */
export const CHAPTER_SWITCHES = 10;
/** 输入多少个字。 */
export const INPUT_CHARS = 100;
/** 检索用的查询词。 */
export const SEARCH_QUERY = "水声";

/** 跑完整段序列；任何一项失败都只记一条"没量到"，不中断后面的项。 */
export async function runProbe(deps: ProbeDeps): Promise<ProbeStep[]> {
  const steps: ProbeStep[] = [];

  // ① 切章：连续切 10 次（两章来回），报平均与最差
  const chapters = deps.chapters();
  if (chapters.length >= 2) {
    try {
      const times: number[] = [];
      for (let index = 0; index < CHAPTER_SWITCHES; index += 1) {
        const started = deps.now();
        await deps.openChapter(chapters[index % 2]);
        await deps.frame();
        times.push(deps.now() - started);
      }
      const average = times.reduce((sum, value) => sum + value, 0) / times.length;
      const worst = Math.max(...times);
      steps.push({
        name: "界面·切章",
        ms: average,
        note: `${CHAPTER_SWITCHES} 次平均（最差 ${worst.toFixed(1)} ms；含先落盘再切）`,
      });
    } catch (error) {
      steps.push({ name: "界面·切章", ms: 0, note: `没量到：${String(error)}` });
    }
  } else {
    steps.push({ name: "界面·切章", ms: 0, note: "没量到：这本书只有一章（或没有章）" });
  }

  // ② 输入 100 字 → 等落盘（"打字跟不跟手"的那一段往返）
  try {
    const started = deps.now();
    deps.insertText("测".repeat(INPUT_CHARS));
    await deps.waitSaved();
    steps.push({
      name: "界面·输入 100 字→落盘",
      ms: deps.now() - started,
      note: "插入到落盘完成（含自动落盘的往返）",
    });
  } catch (error) {
    steps.push({ name: "界面·输入 100 字→落盘", ms: 0, note: `没量到：${String(error)}` });
  }

  // ③ 滚动一次
  try {
    const started = deps.now();
    deps.scroll();
    await deps.frame();
    steps.push({ name: "界面·滚动一次", ms: deps.now() - started, note: "滚到底 + 等一帧" });
  } catch (error) {
    steps.push({ name: "界面·滚动一次", ms: 0, note: `没量到：${String(error)}` });
  }

  // ④ 检索：输入 → 防抖 → 命中 → 列表渲染完
  try {
    const started = deps.now();
    await deps.search(SEARCH_QUERY);
    steps.push({
      name: "界面·检索（输入→列表）",
      ms: deps.now() - started,
      note: `查询「${SEARCH_QUERY}」，含 250ms 防抖与列表渲染；与命令行口径是两回事`,
    });
  } catch (error) {
    steps.push({ name: "界面·检索（输入→列表）", ms: 0, note: `没量到：${String(error)}` });
  }

  return steps;
}
