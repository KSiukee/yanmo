// 编辑器会话：**只挂当前章**的编排（边写边存 + 切章 + 关窗闸门 + 光标）。
//
// 从组件里抽出来，是因为它和"怎么排版、长什么样"是两个完全不同的变化理由：
// 以后改排版不该动落盘/切章，改切章也不该动视图。
//
// 三条纪律：
// - **一个编辑器实例从头用到尾**：切章只是换内容 + 换落盘控制器，全本永远不会同时挂进来；
// - 击键只进编辑器，**逐键绝不跨边界**；只有落盘/校验/切章这些低频动作才过边界；
// - 切章、关窗都先落盘：没落干净就不切 / 就不放行。

import { onBeforeUnmount, onMounted, ref, shallowRef, type Ref, type ShallowRef } from "vue";
import { useEditor, type Editor } from "@tiptap/vue-3";
import StarterKit from "@tiptap/starter-kit";

import {
  abandonSession,
  ackCloseRequest,
  armExitGate,
  bodyFingerprint,
  chapterNeighbors,
  closeSession,
  createChapter,
  createWork,
  deleteWork,
  emergencySnapshot,
  emptyTrash,
  exportWork,
  escapeExport,
  listShelf,
  listTrash,
  onCloseRequested,
  openChapter,
  openEditorTarget,
  openWorkTarget,
  purgeNode,
  purgeWork,
  renameWork,
  requestExit,
  restoreNode,
  restoreWork,
  saveBody,
  saveCursor,
  sessionReport,
  type ChapterNeighbors,
  type EditorCursor,
  type EditorSnapshot,
} from "../api/core";
import { Autosave, type AutosaveState } from "./autosave";
import { ChapterSwitch } from "./chapters";
import { useDirectory, type Directory } from "./directory";
import { docToText, textToHtml } from "./doc";
import { ExitGate, type ExitGateState } from "./exitguard";
import { useShelf, type Shelf } from "./shelf";
import { useTrash, type Trash } from "./trash";

export interface EditorSession {
  editor: ShallowRef<Editor | null>;
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
  /** 当前作品 id（书架用来标"正在写这本"） */
  workId: Ref<number | null>;
  persistNow: () => void;
  switchChapter: (node_id: number | null | undefined) => Promise<void>;
  /** 在某一章后面新建一章并切过去（目录树的「+」走这条） */
  addChapterAfter: (node_id: number) => Promise<void>;
  /** 删掉目录里的一段（软删，进回收站；删到正在写的那一支会自动换落点） */
  deleteNode: (node_id: number) => Promise<void>;
  /** 换一本书（`null` = 回到默认落点）；返回是否真的切过去了 */
  switchWork: (work_id: number | null) => Promise<boolean>;
  retryExit: () => void;
  escapeExit: () => void;
  forceExit: () => void;
}

const IDLE: AutosaveState = {
  status: "idle",
  detail: "",
  char_count: 0,
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
  const currentNodeId = ref<number | null>(null);

  const autosave = shallowRef<Autosave | null>(null);
  let gate: ExitGate | null = null;
  let stopCloseListener: (() => void) | null = null;

  const editor = useEditor({
    content: "",
    extensions: [StarterKit],
    // 击键只进这里，不跨进程；真正的落盘由 autosave 按节奏发起
    onUpdate: ({ editor: instance }) => autosave.value?.changed(docToText(instance)),
  });

  // ── 落盘控制器（一章一个，旧的先退场） ─────────────────────────────
  function makeAutosave(snapshot: EditorSnapshot): Autosave {
    autosave.value?.dispose();
    const engine = new Autosave({
      node_id: snapshot.node_id,
      transport: {
        save: saveBody,
        fingerprint: bodyFingerprint,
        emergency: (node_id, body, reason) => emergencySnapshot(node_id, body, reason),
      },
      onState: (next) => {
        saveState.value = next;
      },
    });
    engine.attach(snapshot.body, {
      char_count: snapshot.char_count,
      word_count: snapshot.word_count,
      fingerprint: snapshot.fingerprint,
    });
    autosave.value = engine;
    return engine;
  }

  // ── 换内容：换内容，不换实例 ───────────────────────────────────────
  function applyChapter(snapshot: EditorSnapshot) {
    chapterTitle.value = snapshot.title;
    workId.value = snapshot.work_id;
    currentNodeId.value = snapshot.node_id;
    saveState.value = {
      ...IDLE,
      status: snapshot.fingerprint === "" ? "idle" : "saved",
      char_count: snapshot.char_count,
      word_count: snapshot.word_count,
    };
    // 第二个参数 false：载入内容不算"作者改动"，不触发落盘
    editor.value?.commands.setContent(textToHtml(snapshot.body), false);
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

  /** 落盘 + 记光标：失焦、切后台、切章、关窗前都要来一次（**不跟着击键走**）。 */
  function persistNow() {
    const engine = autosave.value;
    if (!engine) return;
    void engine.flush();
    const cursor = currentCursor();
    if (cursor) void saveCursor(engine.node_id, cursor).catch(() => {});
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
      failure.value = `没能切到那一章：${message}`;
    },
  });

  async function switchChapter(node_id: number | null | undefined) {
    if (!node_id) return;
    switching.value = true;
    failure.value = null;
    try {
      if ((await switcher.to(node_id)) === "switched") await refreshNeighbors();
    } finally {
      switching.value = false;
    }
  }

  /** 在某一章后面新建一章并直接切过去（目录树的「+」与"接着写下一章"走同一条路）。 */
  async function addChapterAfter(node_id: number) {
    if (switcher.switching) return;
    switching.value = true;
    failure.value = null;
    try {
      // 核心的"插在这一章之后"：同级、紧随其后、序号密集、不跨卷
      // 标题留空＝由核心按**同层序号**取名（按全书取号的话，分卷之后会跳号）
      const created = await createChapter(node_id, "");
      if ((await switcher.to(created)) === "switched") await refreshNeighbors();
      await directory.refresh(); // 新章得看得见（目录不为别的动作整树重建）
    } catch (error) {
      failure.value = `没能新建章节：${error instanceof Error ? error.message : String(error)}`;
    } finally {
      switching.value = false;
    }
  }

  // 目录树：只接"看得见、点得动、拖得走"，切章仍走上面那条（先落盘再切）
  const directory = useDirectory({
    workId,
    currentNodeId,
    saveState,
    openChapter: (node_id) => switchChapter(node_id),
    onError: (message) => {
      failure.value = `目录操作没能完成：${message}`;
    },
  });

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
      return true;
    } catch (error) {
      failure.value = `没能换到那本书：${error instanceof Error ? error.message : String(error)}`;
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
        failure.value = `没能先把这一章存下来，这次没有删除：${error instanceof Error ? error.message : String(error)}`;
        return;
      }
    }
    if ((await directory.remove(node_id)) === null) return;
    if (!hitsCurrent) return;
    // 这一支已经不在了：**把落盘控制器摘掉**，否则切换流程还会去给一个已删除的节点记光标，
    // 那一步会报错并把切换整个拦下来（真机上撞出来的第二层）
    autosave.value?.dispose();
    autosave.value = null;
    await switchWork(work); // 回到这本书还活着的那一章
  }

  // 回收站：捞回来 / 彻底删掉；捞回来之后目录树与书架都得跟着刷新
  const trash = useTrash({
    transport: {
      list: listTrash,
      restoreWork,
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
      failure.value = `回收站操作没能完成：${message}`;
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
    },
    workId,
    openWork: (target) => switchWork(target),
    // 删书之前也先把手上这一章落盘：存不下去就不该动手删
    beforeRemove: () => flushCurrent(),
    onError: (message) => {
      failure.value = `书架操作没能完成：${message}`;
    },
  });

  onMounted(async () => {
    window.addEventListener("blur", persistNow);
    document.addEventListener("visibilitychange", onVisibilityChange);
    try {
      const snapshot = await openEditorTarget();
      applyChapter(snapshot);
      makeAutosave(snapshot);
      await refreshNeighbors();

      // 关窗闸门：先落盘，存不下去就别想走
      gate = new ExitGate({
        autosave: () => autosave.value,
        closeSession: (node_id) => closeSession(node_id),
        abandonSession: () => abandonSession(),
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
          : "时间未知";
        crashNotice.value = `上次没有正常退出（最后落盘：${when}）——已回到崩溃前那一章的最后落盘位置。`;
      }
    } catch (error) {
      failure.value = error instanceof Error ? error.message : String(error);
    }
  });

  onBeforeUnmount(() => {
    window.removeEventListener("blur", persistNow);
    document.removeEventListener("visibilitychange", onVisibilityChange);
    stopCloseListener?.();
    void autosave.value?.flush(); // 先发起落盘（已在飞的不受 dispose 影响）
    autosave.value?.dispose();
  });

  return {
    editor,
    addChapterAfter,
    deleteNode,
    directory,
    shelf,
    trash,
    workId,
    switchWork,
    chapterTitle,
    saveState,
    exitState,
    neighbors,
    switching,
    failure,
    crashNotice,
    persistNow,
    switchChapter,
    retryExit: () => void gate?.retry(),
    escapeExit: () => void gate?.escape(),
    forceExit: () => void gate?.forceExit(),
  };
}
