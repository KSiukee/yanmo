// 编辑器会话：**只挂当前章**的编排（边写边存 + 切章 + 关窗闸门 + 光标）。
//
// 从组件里抽出来，是因为它和"怎么排版、长什么样"是两个完全不同的变化理由：
// 以后改排版不该动落盘/切章，改切章也不该动视图。
//
// 三条纪律：
// - **一个编辑器实例从头用到尾**：切章只是换内容 + 换落盘控制器，全本永远不会同时挂进来；
// - 击键只进编辑器，**逐键绝不跨边界**；只有落盘/校验/切章这些低频动作才过边界；
// - 切章、关窗都先落盘：没落干净就不切 / 就不放行。

import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch, type Ref, type ShallowRef } from "vue";
import { useEditor, type Editor } from "@tiptap/vue-3";
import { plainTextExtensions } from "./extensions";

import {
  abandonSession,
  ackCloseRequest,
  armExitGate,
  bodyFingerprint,
  chapterNeighbors,
  readAppearance,
  resetAppearance,
  closeSession,
  createChapter,
  createWork,
  deleteWork,
  emergencySnapshot,
  emptyTrash,
  exportWork,
  escapeExport,
  isFullscreen,
  listShelf,
  listTrash,
  onCloseRequested,
  openChapter,
  openEditorTarget,
  openWorkTarget,
  purgeNode,
  purgeWork,
  namingRewriteApply,
  namingRewritePreview,
  renameWork,
  requestExit,
  restoreNode,
  restorePreview,
  restoreWork,
  saveBody,
  readBackupStatus,
  readRestoreSources,
  previewRestoreSource,
  applyRestore,
  pickRestoreDatabase,
  runBackupNow,
  saveCursor,
  setWorkLanguage,
  setFullscreen,
  diagnoseNote,
  readLocationInfo,
  pickDataDir,
  confirmLocation,
  moveDataDir,
  cancelDataDir,
  writeBackupConfig,
  sessionReport,
  snapshotDiff,
  snapshotDrop,
  snapshotKeep,
  snapshotList,
  snapshotRestore,
  compileOpenFolder,
  compilePresets,
  compilePreview,
  compileWork,
  setNodeSummary,
  setWorkSummary,
  treeSetVolumeTarget,
  typesetApply,
  typesetRules,
  typesetScan,
  writeAppearance,
  writingToday,
  writingOverview,
  type ChapterNeighbors,
  type EditorCursor,
  type EditorSnapshot,
  type SnapshotRestoreAck,
} from "../api/core";
import { Autosave, type AutosaveState } from "./autosave";
import { useAddChapter, type AddChapter } from "./add-chapter";
import { useAppearance, type AppearanceState } from "./appearance";
import { useWriting, type WritingState } from "./writing";
import { localOffsetMinutes, useBackup, type BackupState } from "./backup";
import { ChapterSwitch } from "./chapters";
import { t } from "../locales/index.ts";
import {
  asCaliber,
  asLanguage,
  nextCaliber,
  nextLanguage,
  type Caliber,
  type WorkLanguage,
} from "./wordcount.ts";
import { useDirectory, type Directory } from "./directory";
import { docToText, textToHtml } from "./doc";
import { insertIntoChapter } from "./insert-text";
import { watchFocusAndIme } from "./diagnose";
import { primeImeThen } from "./ime-prime";
import { ExitGate, type ExitGateState } from "./exitguard";
import { focusPlan } from "./focus";
import { useShelf, type Shelf } from "./shelf";
import { useCompile, type CompileState } from "./compile";
import { useChapterNote, type ChapterNote } from "./note";
import { useSnapshots, type Snapshots } from "./snapshots";
import { DEFAULT_QUOTE_STYLE, useTypeset, type TypesetState } from "./typeset";
import { useLocation, type LocationState } from "./location";
import { useRestore, type RestoreState } from "./restore";
import { useTrash, type Trash } from "./trash";
import { attachGlobalKeys } from "./global-keys";
import type { KeyTarget } from "./shortcuts";
import { useZen, type ZenState } from "./zen";
import { useFloatingPanels, type FloatingPanels, type PanelId } from "./panels";

export interface EditorSession {
  editor: ShallowRef<Editor | undefined>;
  chapterTitle: Ref<string>;
  saveState: Ref<AutosaveState>;
  exitState: Ref<ExitGateState>;
  neighbors: Ref<ChapterNeighbors | null>;
  switching: Ref<boolean>;
  failure: Ref<string | null>;
  crashNotice: Ref<string | null>;
  /** 目录树：看得见、点得动、拖得走（切章仍走这里，先落盘再切） */
  directory: Directory;
  /** 书架：多作品是默认形态（切书同样先落盘再切） */
  shelf: Shelf;
  /** 回收站：删错了能捞回来（恢复与真删的语义全在核心） */
  trash: Trash;
  /** 版本历史：这一章留过哪些版本，看看差异、回滚（滚动保留与"回滚先留底"全在核心） */
  snapshots: Snapshots;
  /** 排版清理：先看后改（建议清单在核心扫出来，勾中的那几处才动） */
  typeset: TypesetState;
  /** 当前章的"一句话"（投稿包的大纲要用它）：单独存、单独显示 */
  note: ChapterNote;
  /** 编译：一份原稿 → 一种成品（投稿版 docx / 分章 txt / 合并 txt） */
  compile: CompileState;
  /** 点「+」之后的编排：先问路标，再照作者意图建章（视图只管"点了哪一行"） */
  adding: AddChapter;
  /** 外观 / 写作行为偏好（设置面板用；焦点策略也读它） */
  appearance: AppearanceState;
  /** 码字统计：今日进度（状态栏）与码字日历（面板） */
  writing: WritingState;
  /** 当前作品 id（书架用来标"正在写这本"） */
  workId: Ref<number | null>;
  persistNow: () => void;
  /**
   * 把一段文字插进当前这一章的正文（作答落章走这条）——算一次正常编辑，自动落盘照常。
   * `at`：`cursor` 光标处 / `end` 章末；没在正文里点过时一律按章末（分寸见 [`insertIntoChapter`]）。
   * 返回有没有插进去（没打开任何一章就是 `false`）。
   */
  insertText: (text: string, at: "cursor" | "end") => boolean;
  switchChapter: (node_id: number | null | undefined, fresh?: boolean) => Promise<void>;
  /// 在某一章后面新建一章并切过去（目录树的「+」走这条）；返回新章 id，没建成是 null
  addChapterAfter: (node_id: number) => Promise<number | null>;
  /** 删掉目录里的一段（软删，进回收站；删到正在写的那一支会自动换落点） */
  deleteNode: (node_id: number) => Promise<void>;
  /** 换一本书（`null` = 回到默认落点）；返回是否真的切过去了 */
  switchWork: (work_id: number | null) => Promise<boolean>;
  retryExit: () => void;
  escapeExit: () => void;
  forceExit: () => void;
  /** 作品语言（跟书走） */
  language: Ref<WorkLanguage>;
  /** 落定后的字数口径（作者选过 → 它；没选过 → 作品语言的默认） */
  caliber: Ref<Caliber>;
  /** 状态栏那个数字点一下：换下一个口径并**落库**（没落成就不改显示） */
  cycleCaliber: () => Promise<void>;
  /** 语言按钮点一下：换下一个语言，并按核心给的落定口径刷新 */
  cycleLanguage: () => Promise<void>;
  /** 备份：设置页、自动触发（每日首启 / 关窗）与那条插盘提示 */
  backup: BackupState;
  /** 从备份恢复：列出来源、看清会丢什么、确认换库（换完壳会重启） */
  restore: RestoreState;
  /** 稿子放在哪：首启确认位置、设置里换位置（换完壳会重启；旧位置不删） */
  location: LocationState;
  /** 专注模式：藏两侧栏与顶栏、只留正文（**当下这一会儿**的状态，不落盘；判断在 zen.ts） */
  zen: ZenState;
  /** 全屏开着没有（F11 / Esc 用；只反映窗口真实状态，不落盘） */
  fullscreenOn: Ref<boolean>;
  /** 切全屏：只改窗口，不碰稿子；失败只报一句 */
  toggleFullscreen: () => Promise<void>;
  /** 专注时的悬浮卡片（大纲这类"看一眼就走"的内容）：开着哪一张；不落盘 */
  panels: FloatingPanels;
  /** 点悬浮按钮：开着就收（并把焦点还给正文），收着就开 */
  togglePanel: (id: PanelId) => void;
  /** 收起卡片并**把焦点还给正文**（焦点留在按钮上输入法就没落点，见 focus.ts） */
  closePanel: () => void;
}

const IDLE: AutosaveState = {
  status: "idle",
  detail: "",
  char_count: 0,
  chars_no_punct: 0,
  word_count: 0,
  incident: null,
};

export function useEditorSession(): EditorSession {
  const chapterTitle = ref("");
  const failure = ref<string | null>(null);
  const crashNotice = ref<string | null>(null);
  const saveState = ref<AutosaveState>({ ...IDLE });
  const exitState = ref<ExitGateState>({ blocked: false, message: "", escapePath: null, busy: false });
  const neighbors = ref<ChapterNeighbors | null>(null);
  const switching = ref(false);
  /** 当前作品与当前章——目录树认这两个（换作品换树，落地的那行跟字数） */
  const workId = ref<number | null>(null);
  /** 作品语言（跟书走）与**落定后的**字数口径（作者选过 → 它；没选过 → 语言默认） */
  const language = ref<WorkLanguage>("zh");
  const caliber = ref<Caliber>("chars");
  const currentNodeId = ref<number | null>(null);
  /** 专注模式（不落盘：重开软件回到常规三栏）与全屏（窗口真实状态，启动时对一次表） */
  const zen = useZen();
  const fullscreenOn = ref(false);
  /** 专注时的悬浮卡片（大纲）：一次只开一张 */
  const panels = useFloatingPanels();

  const autosave = shallowRef<Autosave | null>(null);
  let gate: ExitGate | null = null;
  let stopCloseListener: (() => void) | null = null;
  let stopCompositionWatch: (() => void) | null = null;
  /** 启动诊断的清理句柄（焦点/输入法事件监听） */
  let stopDiagnoseWatch: (() => void) | null = null;
  /** 全局快捷键的解绑句柄（键位与分寸见 editor/shortcuts.ts） */
  let stopGlobalKeys: (() => void) | null = null;
  /** "弹窗关掉就把焦点还给正文"的观察者 */
  let stopDialogFocusWatch: (() => void) | null = null;

  const editor = useEditor({
    content: "",
    extensions: plainTextExtensions(),
    // 击键只进这里，不跨进程；真正的落盘由 autosave 按节奏发起
    onUpdate: ({ editor: instance }) => {
      // **输入法组字中途不算改动**：这时正文里还是拼音字母（还没成字），算进字数会让
      // 状态栏"先跳一个数、选完字再刷新"——作者会怀疑字数到底准不准。
      // 组字结束时（compositionend）再按最终正文算一次（见 watchComposition）。
      if (instance.view.composing) return;
      autosave.value?.changed(docToText(instance));
    },
  });

  // ── 落盘控制器（一章一个，旧的先退场） ─────────────────────────────
  function makeAutosave(snapshot: EditorSnapshot): Autosave {
    autosave.value?.dispose();
    const engine = new Autosave({
      node_id: snapshot.node_id,
      transport: {
        // 落盘带上本地时区偏移：核心拿它算"这一笔算哪一天"（账本见 store::writing）
        save: (node_id, body) => saveBody(node_id, body, localOffsetMinutes()),
        fingerprint: bodyFingerprint,
        emergency: (node_id, body, reason) =>
          emergencySnapshot(node_id, body, reason, localOffsetMinutes()),
      },
      onState: (next) => {
        saveState.value = next;
        // 每次落盘成功刷一次"今日"：这一笔刚记进账本（核心那边与正文同一个事务）。
        // 跨零点时也只有下一笔落盘才刷新——不需要额外定时器，作者一动手就是最新的。
        if (next.status === "saved") void writing.refreshToday();
      },
    });
    engine.attach(snapshot.body, {
      char_count: snapshot.char_count,
      chars_no_punct: snapshot.chars_no_punct,
      word_count: snapshot.word_count,
      fingerprint: snapshot.fingerprint,
    });
    autosave.value = engine;
    return engine;
  }

  /** 刚打开的这一章有没有"上次读到哪"的记录——焦点策略要用（见 editor/focus.ts） */
  let openedHadCursor = false;

  // ── 换内容：换内容，不换实例 ───────────────────────────────────────
  function applyChapter(snapshot: EditorSnapshot) {
    openedHadCursor = snapshot.cursor !== null;
    chapterTitle.value = snapshot.title;
    workId.value = snapshot.work_id;
    currentNodeId.value = snapshot.node_id;
    saveState.value = {
      ...IDLE,
      status: snapshot.fingerprint === "" ? "idle" : "saved",
      char_count: snapshot.char_count,
      chars_no_punct: snapshot.chars_no_punct,
      word_count: snapshot.word_count,
    };
    language.value = asLanguage(snapshot.work_language);
    caliber.value = asCaliber(snapshot.word_caliber);
    // 换书就重读目标（每本书可以各设各的）与偏好（每书覆盖项），并刷今日
    void appearance.loadWork();
    void writing.loadGoal();
    void writing.refreshToday();
    // "一句话"跟着章走：核心给什么就是什么（编辑器里没存的草稿随切章丢掉）
    note.reset(snapshot.summary);
    // emitUpdate: false —— 载入内容不算"作者改动"，不触发落盘
    editor.value?.commands.setContent(textToHtml(snapshot.body), { emitUpdate: false });
    applyCursor(snapshot.cursor);
  }

  function scrollArea(): HTMLElement | null {
    const node = editor.value?.view.dom.closest(".editor__area");
    return node instanceof HTMLElement ? node : null;
  }

  /** 回到上次读到的位置（位置越界就夹进文档范围——记录可能来自更长的旧版本）。 */
  function applyCursor(cursor: EditorCursor | null) {
    const instance = editor.value;
    if (!cursor || !instance) return;
    const size = instance.state.doc.content.size;
    const clamp = (value: number) => Math.max(0, Math.min(value, size));
    instance.commands.setTextSelection({ from: clamp(cursor.anchor), to: clamp(cursor.head) });
    requestAnimationFrame(() => {
      const area = scrollArea();
      if (area) area.scrollTop = cursor.scroll_top; // 等一帧：内容刚换完，可滚动高度还没稳定
    });
  }

  function currentCursor(): EditorCursor | null {
    const instance = editor.value;
    if (!instance) return null;
    return {
      anchor: instance.state.selection.anchor,
      head: instance.state.selection.head,
      scroll_top: Math.round(scrollArea()?.scrollTop ?? 0),
    };
  }

  /** 只落盘，不等光标：删东西之前要把手上这一章存下来（存不下就别删）。 */
  async function flushCurrent(): Promise<void> {
    const engine = autosave.value;
    if (engine) await engine.flush();
  }

  /**
   * 落盘，但**不把失败抛给调用方**——只用于"顺手存一下"的场合（失焦、切后台、Ctrl+S）：
   * 那些地方失败已经反映在状态栏（autosave 的 error 态与 detail）里。
   *
   * 反之，"先落盘再动手"的六道守卫一律 `await flushCurrent()`：它们**必须**拿到失败并拦住动作。
   */
  function flushQuietly(): void {
    void flushCurrent().catch(() => {});
  }

  /**
   * 组字收尾时补算一次。
   *
   * 为什么需要"补"：WebView2 在 `contentEditable` 上的输入法集成并不总是按预期发 update
   * （上游有未修的报告），只在 `onUpdate` 里躲开组字态，可能出现"最后一次没算上"。
   * 这里显式补一刀——`changed` 对相同文本是幂等的（内容没变直接返回），重复调用无害。
   */
  function watchComposition(): () => void {
    const dom = editor.value?.view.dom;
    if (!dom) return () => {};
    const onEnd = () => {
      const instance = editor.value;
      if (instance) autosave.value?.changed(docToText(instance));
    };
    dom.addEventListener("compositionend", onEnd);
    return () => dom.removeEventListener("compositionend", onEnd);
  }

  /**
   * 等窗口**真的被激活**，再决定编辑器的初始焦点。
   *
   * 为什么等：Windows 的输入法是随"窗口被激活"挂到输入元素上的。窗口还藏着的时候就把
   * 编辑器聚焦了，Win10 上会出现「中文输入法点不出来、要先切英文打几个字母再切回来」；
   * 同样的代码在 Win11 上恰好不露。最多等 3 秒——**别为了输入法把启动卡住**；
   * 到点还没等到也照常聚焦（页内聚焦不会去抢别的窗口）。
   */
  /**
   * 现在有没有弹窗盖在界面上（首启引导、书架、回收站、设置、备份、恢复、快照、排版、码字日历、删章路标）。
   *
   * 为什么要有这个：Windows 的输入法是随"焦点落到输入元素"挂上去的，而**弹窗会抢走焦点**。
   * 启动时若正弹着首启引导就急着给正文定焦点，焦点会被弹窗拿走；等弹窗关掉又没人把焦点还回来——
   * 表现就是"中文输入法点不出来，要切英文打几个字母再切回来"。所以：**弹窗盖着就先不定焦点，
   * 等它关掉再定**（见 anyDialogOpen 的 watch）。
   */
  const anyDialogOpen = computed(
    () =>
      shelf.visible.value ||
      trash.visible.value ||
      snapshots.visible.value ||
      typeset.visible.value ||
      appearance.visible.value ||
      writing.visible.value ||
      backup.visible.value ||
      restore.visible.value ||
      location.visible.value,
  );

  function whenWindowFocused(): Promise<void> {
    if (document.hasFocus()) return Promise.resolve();
    return new Promise((resolve) => {
      let timer = 0;
      const finish = () => {
        window.removeEventListener("focus", finish);
        window.clearTimeout(timer);
        resolve();
      };
      window.addEventListener("focus", finish);
      timer = window.setTimeout(finish, 3000);
    });
  }

  /** 落盘 + 记光标：失焦、切后台、切章、关窗前都要来一次（**不跟着击键走**）。 */
  function persistNow() {
    const engine = autosave.value;
    if (!engine) return;
    // 顺手存一下：失败会亮在状态栏；拦住动作是那六道守卫的事（它们 await flushCurrent）
    flushQuietly();
    const cursor = currentCursor();
    if (cursor) void saveCursor(engine.node_id, cursor).catch(() => {});
  }

  /** 插字那点分寸在 [`insertIntoChapter`]；这里只回答"有没有打开的章、有没有光标"。 */
  function insertText(text: string, at: "cursor" | "end"): boolean {
    const instance = editor.value;
    if (!instance || currentNodeId.value === null) return false;
    return insertIntoChapter(instance, text, at, instance.isFocused || openedHadCursor);
  }

  /**
   * 状态栏那个数字点一下：换个口径，写进**全局偏好**（与设置面板同一份）。
   *
   * 写到全局而不是每本书：第一版界面只暴露全局那份（见外观设置骨架那条任务），
   * 每本书的覆盖留给将来的"这本书单独一套"。**写成功才改显示**——存不下去就别装作换了。
   */
  async function cycleCaliber(): Promise<void> {
    const wanted = nextCaliber(caliber.value);
    try {
      await writeAppearance(null, { word_count_caliber: wanted });
      caliber.value = wanted;
    } catch (error) {
      failure.value = error instanceof Error ? error.message : String(error);
    }
  }

  /**
   * 语言按钮点一下：换作品语言。
   *
   * 口径**不在这里自己算**——核心把"落定后的口径"一起回传（作者从没选过口径时，
   * 换成英文作品就该变成按词）。界面抄一份"语言 → 口径"的对照表，两边迟早走偏。
   */
  async function cycleLanguage(): Promise<void> {
    const work = workId.value;
    if (work === null) return;
    try {
      const ack = await setWorkLanguage(work, nextLanguage(language.value));
      language.value = asLanguage(ack.language);
      caliber.value = asCaliber(ack.word_caliber);
    } catch (error) {
      failure.value = error instanceof Error ? error.message : String(error);
    }
  }

  function onVisibilityChange() {
    if (document.visibilityState === "hidden") persistNow();
  }

  async function refreshNeighbors() {
    const engine = autosave.value;
    if (!engine) return;
    try {
      neighbors.value = await chapterNeighbors(engine.node_id);
    } catch {
      neighbors.value = null; // 导航拿不到不影响写作
    }
  }

  /**
   * 回滚把正文换成了旧版本：**换内容不换实例**（与切章同一条纪律），
   * 并让落盘控制器重建比对基准——否则下一次读回校验会以为"库被人改了"，白抢救一次。
   */
  function applyRestored(ack: SnapshotRestoreAck) {
    if (currentNodeId.value !== ack.node_id) return; // 回滚途中切了章：不动现在这一章
    editor.value?.commands.setContent(textToHtml(ack.body), { emitUpdate: false });
    autosave.value?.attach(ack.body, {
      char_count: ack.char_count,
      chars_no_punct: ack.chars_no_punct,
      word_count: ack.word_count,
      fingerprint: ack.fingerprint,
    });
    void directory.refresh(); // 字数变了：目录里那行小字要跟上
    void refreshNeighbors();
  }

  /**
   * 排版清理改完了正文：**换内容不换实例**（与切章、回滚同一条纪律），
   * 并立刻落盘——清理是一步低频动作，等得起，也让"改完就是存好的"。
   */
  async function applyTypeset(text: string) {
    editor.value?.commands.setContent(textToHtml(text), { emitUpdate: false });
    autosave.value?.changed(text);
    await flushCurrent();
    void directory.refresh(); // 字数变了：目录里那行小字要跟上
    void refreshNeighbors();
  }

  /**
   * 打开一章之后把光标安排明白：**历史章挪出正文、该写的章接着写**。
   *
   * 放在"邻居读回来之后"再判：要知道这一章是不是全书最后一章（跨卷按阅读顺序）。
   * `fresh` = 刚新建/补写出来的那一种（作者的意图就是要写）。
   */
  function settleFocus(fresh: boolean) {
    const instance = editor.value;
    const order = neighbors.value;
    if (!instance || !order) return; // 拿不到阅读顺序就不动光标（安全那一边）
    const plan = focusPlan({
      fresh,
      had_cursor: openedHadCursor,
      is_latest: order.index === order.total,
      // 偏好还没读回来时按"不抢焦点"处理——宁可少聚焦一次，也不要在历史章里插进乱字符
      jump_to_end: appearance.values.value?.jump_to_end_on_latest ?? false,
    });
    if (plan === "focus-end") instance.commands.focus("end");
    // 位置已经由 applyCursor 放好了，这里只把**焦点**交进正文——
    // 输入法要的是"焦点落在可编辑元素上"，没有这个，按热键也没人接（见 focus.ts 的说明）
    else if (plan === "restore") instance.commands.focus();
    else if (plan === "blur") instance.commands.blur();
  }

  const switcher = new ChapterSwitch({
    autosave: () => autosave.value,
    currentCursor: () => currentCursor(),
    saveCursor: (node_id, cursor) => saveCursor(node_id, cursor),
    loadChapter: (node_id) => openChapter(node_id),
    applyChapter: (snapshot) => applyChapter(snapshot),
    startAutosave: (snapshot) => {
      makeAutosave(snapshot);
    },
    onError: (message) => {
      failure.value = t("session.switch_failed", { detail: message });
    },
  });

  async function switchChapter(node_id: number | null | undefined, fresh = false) {
    if (!node_id) return;
    switching.value = true;
    failure.value = null;
    try {
      if ((await switcher.to(node_id)) === "switched") {
        await refreshNeighbors();
        settleFocus(fresh);
        // 版本历史跟着换章：开着面板时不能还摆着上一章的版本
        if (snapshots.visible.value) void snapshots.refresh();
      }
    } finally {
      switching.value = false;
    }
  }

  /**
   * 全屏：**只改窗口，不碰稿子**。
   *
   * 为什么不做成偏好：全屏是"当下这一会儿"的窗口状态，跟专注模式一样不落盘——
   * 重开软件回到常规窗口，作者不会遇到"一打开就是全屏、找不到退出"的尴尬。
   * 状态以**窗口的真实值**为准（写完读回），界面那份 ref 就不会慢慢漂开。
   */
  async function toggleFullscreen(): Promise<void> {
    try {
      fullscreenOn.value = await setFullscreen(!fullscreenOn.value);
    } catch (error) {
      failure.value = t("session.fullscreen_failed", {
        detail: error instanceof Error ? error.message : String(error),
      });
    }
  }

  /** Esc 的语义：回到常规——专注与全屏哪个开着收哪个（两下都收干净）。 */
  function exitFocus(): void {
    zen.exit();
    if (!fullscreenOn.value) return;
    void setFullscreen(false)
      .then((on) => {
        fullscreenOn.value = on;
      })
      .catch(() => {
        fullscreenOn.value = false; // 读不回来就按"已经不在全屏"处理，别把状态卡住
      });
  }

  /** 收起悬浮卡片，并把**焦点还给正文**：焦点留在按钮上的话，输入法就没有落点
   *  （作者看到的现象是"打不出中文"——这条在 focus.ts 里踩过，别再踩第二遍）。 */
  function closePanel(): void {
    if (panels.open.value === null) return;
    panels.close();
    editor.value?.commands.focus();
  }

  /** 点悬浮按钮：开着就收，收着就开（一次只开一张）。 */
  function togglePanel(id: PanelId): void {
    if (panels.isOpen(id)) {
      closePanel();
      return;
    }
    panels.open.value = id;
  }

  /// 在某一章后面新建一章并直接切过去（目录树的「+」与"接着写下一章"走同一条路）。
  ///
  /// **返回新章 id**：目录树连点同一个「+」时要靠它接着往下排（见 `editor/add-chapter.ts`）。
  async function addChapterAfter(node_id: number): Promise<number | null> {
    if (switcher.switching) return null;
    switching.value = true;
    failure.value = null;
    try {
      // 标题留空＝由核心按**同层序号**取名（按全书取号的话，分卷之后会跳号）；
      // 落点也由核心定：编号能认出来就**按号归位**，认不出来才退回"插在点的那一行之后"
      // （见 store::node_edit::add_chapter_after 的说明）
      const created = await createChapter(node_id, "");
      if ((await switcher.to(created)) === "switched") {
        await refreshNeighbors();
        settleFocus(true); // 刚新建：接着写（不管它是第几章）
      }
      await directory.refresh(); // 新章得看得见（目录不为别的动作整树重建）
      return created.node_id; // 新章 id：目录树连点「+」时要靠它接着往下排
    } catch (error) {
      failure.value = t("session.create_chapter_failed", {
        detail: error instanceof Error ? error.message : String(error),
      });
      return null;
    } finally {
      switching.value = false;
    }
  }

  /**
   * 换一本书：落点是那本书上次写的那一章。
   *
   * 复用的是**同一条切章纪律**（先落盘、记光标，再换内容换控制器）——
   * 换书在作者看来只是"换一章"，不该有第二套流程。
   */
  async function switchWork(work_id: number | null): Promise<boolean> {
    if (switcher.switching) return false;
    switching.value = true;
    failure.value = null;
    try {
      const snapshot = work_id === null ? await openEditorTarget() : await openWorkTarget(work_id);
      if ((await switcher.to(snapshot)) === "blocked") return false;
      await refreshNeighbors();
      settleFocus(false);
      if (snapshots.visible.value) void snapshots.refresh();
      return true;
    } catch (error) {
      failure.value = t("session.switch_work_failed", {
        detail: error instanceof Error ? error.message : String(error),
      });
      return false;
    } finally {
      switching.value = false;
    }
  }

  /**
   * 删掉目录里的一段（软删，能捞回来）。
   *
   * 删的要是**正在写的那一支**（它自己或它的上级），得先换个落点——
   * 否则编辑器攥着一个已经进回收站的节点，下一次落盘就会撞墙。
   */
  async function deleteNode(node_id: number): Promise<void> {
    const current = currentNodeId.value;
    const hitsCurrent = current !== null && directory.contains(node_id, current);
    const work = workId.value;
    if (hitsCurrent) {
      // 删的正是手上这一支：**先落盘再删**。反过来做的话，切换流程里的"先落盘"会撞上
      // "节点已删除"，人就卡在一个已经进回收站的章节上了（这条是真机上撞出来的）
      try {
        await flushCurrent();
      } catch (error) {
        failure.value = t("session.delete_unsaved", {
          detail: error instanceof Error ? error.message : String(error),
        });
        return;
      }
    }
    if ((await directory.remove(node_id)) === null) return;
    if (!hitsCurrent) return;
    // 这一支已经不在了：**把落盘控制器摘掉**（`detach`，不是 `dispose` + 置空）——
    // 摘掉是为了让切换流程不去给一个已删除的节点记光标（那一步会报错并把切换整个拦下来，
    // 真机上撞出来的第二层）；**但对象必须留着**：置空会被当成"编辑器还没挂上、没什么可丢的"，
    // 于是"能打字、却永远不会落盘"变成一个静默状态，退出闸门还会放行
    //（2026-09-15 代码质量评审：严重 6）。留着一个已摘下的控制器：状态栏亮红字、
    // flush 必然失败、闸门因此拦得住人。
    autosave.value?.detach();
    await switchWork(work); // 回到这本书还活着的那一章（成功时会建一个新的落盘控制器）
  }

  /**
   * 装配：**有先后要求的创建都收在这一处**，顺序一眼可见。
   *
   * 为什么单列：依赖顺序是隐式的——编译器看不见、单测也碰不到；把新组合式插错位置，
   * 挂载时会直接抛错，真机上就是白屏（这条踩过一次）。约定：
   * ① `directory` / `appearance` 先行（后面的要靠它们）；
   * ② 再建 `adding`（它要 directory + 建章那条路）；
   * ③ `trash` / `shelf` / `snapshots` / `restore` 最后（它们只在回调里互相引用，运行时才碰）。
   */
  function createParts() {
    /** 打开"刚新建/补写"的那一章——要接着写（走同一条切章纪律，只是焦点策略不同） */
    const openFreshChapter = (node_id: number) => switchChapter(node_id, true);

    // 目录树：只接"看得见、点得动、拖得走"，切章仍走上面那条（先落盘再切）
    const directory = useDirectory({
      workId,
      currentNodeId,
      saveState,
      openChapter: (node_id) => switchChapter(node_id),
      openFresh: openFreshChapter,
      onError: (message) => {
        failure.value = t("session.directory_failed", { detail: message });
      },
    });

    // 外观 / 写作行为偏好：全局一份（默认值只在核心那一处）；会话启动时读一次
    const appearance = useAppearance({
      transport: {
        read: readAppearance,
        write: writeAppearance,
        reset: resetAppearance,
        previewNaming: namingRewritePreview,
        applyNaming: namingRewriteApply,
      },
      // "只设这本书"要知道当前是哪一本（workId 在下面才声明，所以这里用取值函数）
      workId,
      onError: (message) => {
        failure.value = t("session.settings_failed", { detail: message });
      },
    });

    // 码字统计：今日进度 + 码字日历（账本在核心；今日跟着落盘走，见 makeAutosave）
    const writing = useWriting({
      transport: {
        today: writingToday,
        overview: writingOverview,
        readGoal: readAppearance,
        writeGoal: writeAppearance,
        tzOffsetMinutes: localOffsetMinutes,
      },
      workId,
      onError: (message) => {
        failure.value = t("session.writing_failed", { detail: message });
      },
    });

    // 备份：把库的一致性快照写到作者指定的几处（快照/体检/保留/账本都在核心）
    const backup = useBackup({
      transport: { status: readBackupStatus, write: writeBackupConfig, run: runBackupNow },
      tzOffsetMinutes: localOffsetMinutes,
      onError: (message) => {
        failure.value = t("session.backup_failed", { detail: message });
      },
    });

    // 点「+」之后的编排（先问路标 → 补写 / 接着建章）：**不放在视图里**，视图只管"点了哪一行"
    const adding = useAddChapter({ directory, addChapterAfter, openFreshChapter });

    // 回收站：捞回来 / 彻底删掉；捞回来之后目录树与书架都得跟着刷新
    const trash = useTrash({
      transport: {
        list: listTrash,
        restoreWork,
        preview: restorePreview,
        restoreNode,
        purgeWork,
        purgeNode,
        empty: emptyTrash,
      },
      workId,
      reopen: async () => {
        await switchWork(null);
      },
      onChanged: () => {
        void shelf.refresh();
        void directory.refresh();
      },
      onError: (message) => {
        failure.value = t("session.trash_failed", { detail: message });
      },
    });

    // 书架：列书 / 建书 / 改名 / 删书；"切书"仍走上面那条（先落盘再切）
    const shelf = useShelf({
      transport: {
        list: listShelf,
        create: createWork,
        rename: renameWork,
        remove: deleteWork,
        export: exportWork,
        writeSummary: setWorkSummary,
        // 命名规则就是外观偏好里那一项，写到"这本书"那一层（每书覆盖）
        writeNaming: (work_id, naming) => writeAppearance(work_id, { naming }).then(() => undefined),
        // 建书页填的「一卷大概多少章」：写进这本书的分卷口径（只影响提示，不改结构）
        writeVolumeTarget: treeSetVolumeTarget,
      },
      workId,
      openWork: (target) => switchWork(target),
      // 删书之前也先把手上这一章落盘：存不下去就不该动手删
      beforeRemove: () => flushCurrent(),
      onError: (message) => {
        failure.value = t("session.shelf_failed", { detail: message });
      },
    });

    // 版本历史：列 / 比 / 留一版 / 删一版 / 回滚（滚动保留、回滚先留底全在核心）。
    // 回滚前先落盘：**存不下去就不覆盖**——这条纪律与"删章之前先存"是同一条。
    const snapshots = useSnapshots({
      transport: {
        list: snapshotList,
        diff: snapshotDiff,
        keep: snapshotKeep,
        drop: snapshotDrop,
        restore: snapshotRestore,
      },
      nodeId: currentNodeId,
      beforeRestore: () => flushCurrent(),
      onRestored: (ack) => applyRestored(ack),
      onError: (message) => {
        failure.value = t("session.snapshots_failed", { detail: message });
      },
    });

    // 排版清理：先扫一遍给作者看，勾了才改（规则与"该改哪一处"全在核心）。
    // **动手前先留一版**：与"回滚先留底"同一条纪律——留不下就绝不改正文。
    const typeset = useTypeset({
      transport: { rules: typesetRules, scan: typesetScan, apply: typesetApply },
      currentText: () => (editor.value ? docToText(editor.value) : ""),
      savedQuoteStyle: () => appearance.values.value?.quote_style ?? DEFAULT_QUOTE_STYLE,
      onQuoteStyle: (style) => {
        void appearance.setQuoteStyle(style);
      },
      beforeApply: async () => {
        await flushCurrent();
        const node_id = currentNodeId.value;
        if (node_id !== null) await snapshotKeep(node_id);
      },
      onApplied: (text) => applyTypeset(text),
      onError: (message) => {
        failure.value = t("session.typeset_failed", { detail: message });
      },
    });

    // 从备份恢复：列来源 / 体检预览 / 换库。**换库前先落盘**——万一没换成、原库回滚，
    // 作者这一章不至于留个缺口；真正动文件的是壳与核心，这里只管叫它。
    const restore = useRestore({
      transport: {
        sources: readRestoreSources,
        preview: previewRestoreSource,
        apply: applyRestore,
        pick: pickRestoreDatabase,
      },
      tzOffsetMinutes: localOffsetMinutes,
      beforeApply: () => flushCurrent(),
      onError: (message) => {
        failure.value = t("session.restore_failed", { detail: message });
      },
    });

    // 稿子放在哪：首启确认 + 设置里换位置。**搬家前先落盘**——手上这一章存不下去就绝不搬；
    // 真正动文件的是壳与核心（复制、核对、写记录），这里只管叫它。
    const location = useLocation({
      transport: {
        info: readLocationInfo,
        pick: pickDataDir,
        confirm: confirmLocation,
        move: moveDataDir,
        cancel: cancelDataDir,
      },
      beforeMove: () => flushCurrent(),
      onError: (message) => {
        failure.value = t("session.location_failed", { detail: message });
      },
    });

    // 编译：先说清会生成哪些文件，再动手（渲染在核心，落盘在壳）。
    const compile = useCompile({
      transport: {
        presets: compilePresets,
        preview: compilePreview,
        run: compileWork,
        openFolder: compileOpenFolder,
      },
      onError: (message) => {
        failure.value = t("session.compile_failed", { detail: message });
      },
    });

    // 当前章的"一句话"：单独存、单独显示（状态机在 editor/note.ts）。
    // **不跟正文一起落盘**——它不参与防抖与指纹校验，存不下去也只报一句错。
    const note = useChapterNote({
      transport: { save: setNodeSummary },
      nodeId: currentNodeId,
      onError: (message) => {
        failure.value = t("session.note_failed", { detail: message });
      },
    });

    return {
      directory,
      appearance,
      writing,
      backup,
      restore,
      location,
      adding,
      trash,
      shelf,
      snapshots,
      typeset,
      note,
      compile,
    };
  }

  // 装配一次，之后各处只用解出来的这几个（顺序约定见 createParts）
  const {
    directory,
    appearance,
    writing,
    backup,
    restore,
    location,
    adding,
    trash,
    shelf,
    snapshots,
    typeset,
    note,
    compile,
  } = createParts();

  onMounted(async () => {
    window.addEventListener("blur", persistNow);
    document.addEventListener("visibilitychange", onVisibilityChange);
    // 全局快捷键：一处定义、一处接线（键位与"不抢键"的分寸见 editor/shortcuts.ts）。
    // 挂 window 的**捕获阶段**：命中就在编辑器之前拦下，免得同一组合键被处理两遍。
    stopGlobalKeys = attachGlobalKeys(
      {
        dialogOpen: () => anyDialogOpen.value,
        zenOn: () => zen.on.value,
        fullscreenOn: () => fullscreenOn.value,
        panelOpen: () => panels.open.value !== null,
        closePanel,
        toggleZen: () => {
          zen.toggle();
        },
        toggleFullscreen: () => void toggleFullscreen(),
        exitFocus,
        prevChapter: () => void switchChapter(neighbors.value?.previous?.id),
        nextChapter: () => void switchChapter(neighbors.value?.next?.id),
        saveNow: () => flushQuietly(),
        // 与目录树的「+」**同一条路**：在当前章后面建一章并直接开写（不另开确认流程）
        newChapter: () => {
          const node_id = currentNodeId.value;
          if (node_id !== null) void addChapterAfter(node_id);
        },
        openSettings: () => void appearance.open(),
        openShelf: () => shelf.toggle(),
      },
      window as unknown as KeyTarget,
    );
    // 窗口真实状态对一次表（不卡启动：读不回来就按"没全屏"处理）。
    // 正常启动都是 false，但万一全屏进来，Esc 才知道该退什么。
    void isFullscreen()
      .then((on) => {
        fullscreenOn.value = on;
      })
      .catch(() => {});
    try {
      await appearance.load(); // 先读偏好：焦点策略要用（读失败按"不抢焦点"走）
      // 备份：读现状 + 今天还没备份过就自动做一次（**不 await**：别拖慢开窗能写字的时间）
      void backup.onStart();
      // 稿子放在哪：读一次现状；壳说"第一次用"才把首启引导亮出来（不 await，同上）
      void location.load();
      const snapshot = await openEditorTarget();
      applyChapter(snapshot);
      makeAutosave(snapshot);
      // 书架列表：**打开当前章之后再拉**（顺序要紧）。正文区那条"第一次使用？"的提示
      // 要拿它判断，而首启那本无名空壳正是上一步（openEditorTarget）才建出来的——
      // 早拉一步只会拿到空列表，提示就永远不出现（真机上就是这么翻的车）。
      void shelf.refresh();
      stopCompositionWatch = watchComposition();
      // 启动诊断：盯着"焦点在不在正文里""输入法有没有组字"（壳没开诊断时这些上报是空转）
      const dom = editor.value?.view.dom;
      if (dom) stopDiagnoseWatch = watchFocusAndIme(dom as HTMLElement);
      await refreshNeighbors();
      // 先等窗口被激活再定初始焦点：见 whenWindowFocused 的说明（Win10 输入法）。
      // 但**弹窗盖着的时候不定**——那会把焦点白送给弹窗，等它关掉又没人还回来（见 anyDialogOpen）。
      await whenWindowFocused();
      // 输入法预热：WebView2 在正文上第一次可能挂不上输入法（上游缺陷，真机日志钉死过）。
      // 先用一个隐藏的真输入框把输入法引到页面上，再交焦点给正文。
      if (!anyDialogOpen.value) {
        primeImeThen(
          () => settleFocus(false),
          (text) => void diagnoseNote(text).catch(() => {}),
        );
      } else {
        settleFocus(false);
      }
      // 弹窗一关就把焦点还给正文（正文已经拿着焦点时不动，免得跟切章/新建的焦点计划打架）
      stopDialogFocusWatch = watch(anyDialogOpen, (open) => {
        if (open) return;
        const dom = editor.value?.view.dom;
        if (dom && document.activeElement === dom) return;
        settleFocus(false);
      });

      // 关窗闸门：先落盘，存不下去就别想走
      gate = new ExitGate({
        autosave: () => autosave.value,
        closeSession: async (node_id) => {
          await closeSession(node_id);
          backup.onClose(); // 异步做一次备份，**不拖慢退出**
        },
        abandonSession: async () => {
          await abandonSession();
          backup.onClose();
        },
        exitApp: () => requestExit(),
        escapeExport: (node_id, body) => escapeExport(node_id, body).then((ack) => ack.path),
        currentBody: () => (editor.value ? docToText(editor.value) : ""),
        onState: (next) => {
          exitState.value = next;
        },
      });
      stopCloseListener = await onCloseRequested(() => {
        // 先回话：告诉壳"通知收到了"——否则壳会以为界面已死，到期直接退出
        void ackCloseRequest().catch(() => {});
        persistNow(); // 关窗前把光标也记下（正文由闸门的 flush 负责）
        void gate?.requestExit();
      });
      // 界面就绪：从现在起关窗会先过闸门（未就绪时一律放行，免得窗口关不掉）
      await armExitGate();

      const notice = await sessionReport();
      if (notice.unclean) {
        const when = notice.last_seen_at
          ? new Date(notice.last_seen_at).toLocaleString()
          : t("session.time_unknown");
        crashNotice.value = t("session.crash_notice", { when });
      }
    } catch (error) {
      failure.value = error instanceof Error ? error.message : String(error);
    }
  });

  onBeforeUnmount(() => {
    window.removeEventListener("blur", persistNow);
    document.removeEventListener("visibilitychange", onVisibilityChange);
    stopGlobalKeys?.();
    stopCloseListener?.();
    stopCompositionWatch?.();
    stopDiagnoseWatch?.();
    stopDialogFocusWatch?.();
    // 卸挂之前把手上这一版落下去，**落完再 dispose**。
    // 为什么顺序要紧：`dispose()` 会让 `saveNow` 直接返回，先 dispose 就等于把
    // "再排一笔"的机会掐掉——在飞的那一笔写的是更早的文本，最后几次击键就没了。
    const engine = autosave.value;
    if (engine) {
      void engine.flush().catch(() => {}).finally(() => engine.dispose());
    }
  });

  return {
    editor,
    addChapterAfter,
    deleteNode,
    directory,
    adding,
    appearance,
    writing,
    shelf,
    trash,
    snapshots,
    typeset,
    note,
    compile,
    workId,
    switchWork,
    backup,
    restore,
    location,
    language,
    caliber,
    cycleCaliber,
    cycleLanguage,
    zen,
    fullscreenOn,
    toggleFullscreen,
    panels,
    togglePanel,
    closePanel,
    chapterTitle,
    saveState,
    exitState,
    neighbors,
    switching,
    failure,
    crashNotice,
    persistNow,
    insertText,
    switchChapter,
    retryExit: () => void gate?.retry(),
    escapeExit: () => void gate?.escape(),
    forceExit: () => void gate?.forceExit(),
  };
}
