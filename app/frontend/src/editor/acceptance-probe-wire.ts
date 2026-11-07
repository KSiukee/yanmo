// 界面级验收的**真接线**：把会话里的动作接进 `acceptance-probe.ts` 的编排，再把结果交给壳。
//
// 与编排分两个文件的原因：编排要能在 node 里单测，而这里 import 的是 Tauri 通道与会话类型。
// 正常启动什么都做不了：壳那边探测回 `false`，第一句就返回。

import { acceptanceProbeActive, acceptanceUiProbe } from "../api/core";
import type { EditorSession } from "./session";
import { runProbe, type ProbeDeps } from "./acceptance-probe";

/** 自测要用到的会话部件（只要这几样，不必等整个会话装配完）。 */
export type ProbeSession = Pick<EditorSession, "directory" | "editor" | "saveState" | "search">;

/** 等一个条件成立（轮询 50ms；到点不管成没成都返回，绝不挂死验收）。 */
function waitFor(predicate: () => boolean, timeoutMs: number): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  return new Promise((resolve) => {
    const check = () => {
      if (predicate() || Date.now() > deadline) {
        resolve();
        return;
      }
      setTimeout(check, 50);
    };
    check();
  });
}

/** 两次 rAF：第一帧提交、第二帧布局完成。 */
function nextFrame(): Promise<void> {
  return new Promise((resolve) => {
    let done = false;
    const finish = () => {
      if (done) return;
      done = true;
      resolve();
    };
    requestAnimationFrame(() => requestAnimationFrame(finish));
    // 兜底：窗口万一不可见，rAF 会暂停——**绝不把验收挂在这儿**（100ms 后强行往下走）
    setTimeout(finish, 100);
  });
}

/**
 * 验收模式下跑一段界面自测并把结果交给壳；**正常启动什么都不做**。
 *
 * 失败一律吞掉：验收模式也要能正常就绪、正常退出——量不到数字比卡死窗口好查得多。
 */
export async function connectAcceptanceProbe(session: ProbeSession): Promise<void> {
  let active = false;
  try {
    active = await acceptanceProbeActive();
  } catch {
    return;
  }
  if (!active) return;

  // 等窗口真的显示：隐藏窗口里 rAF 会被暂停，切章/滚动的"等一帧"会变成等超时
  await waitFor(() => document.visibilityState === "visible", 5000);

  const deps: ProbeDeps = {
    chapters: () =>
      session.directory.rows.value
        .filter((row) => row.kind === "chapter")
        .map((row) => row.id)
        .slice(0, 20),
    openChapter: (nodeId) => session.directory.select(nodeId),
    insertText: (text) => {
      session.editor.value?.commands.insertContent(text);
    },
    waitSaved: () => waitFor(() => session.saveState.value.status === "saved", 5000),
    scroll: () => {
      const dom = session.editor.value?.view.dom.closest(".editor__area");
      if (dom) dom.scrollTop = dom.scrollHeight;
    },
    search: async (query) => {
      if (!session.search.visible.value) session.search.toggle();
      session.search.type(query);
      await waitFor(
        () => !session.search.busy.value && session.search.hits.value.length > 0,
        5000,
      );
    },
    frame: nextFrame,
    now: () => performance.now(),
  };

  try {
    const steps = await runProbe(deps);
    await acceptanceUiProbe(steps);
  } catch {
    // 自测失败不许影响就绪：壳那边照样会写报告（少这几条而已）
  }
}
