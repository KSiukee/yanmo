// 来源观测的 **DOM 那一半**：只听三件事，别的一概不碰。
//
// - `keydown`：落一个字的键（见 `isTypingKey`）；输入法组字时 key 是 `Process`，
//   不算打字，改由 `compositionstart` 报；
// - `compositionstart`：输入法开始组字（中文写作里这就是"打字"）；
// - `paste` / `drop`：整段东西进来（**捕获阶段**听：ProseMirror 可能先处理它，
//   冒泡到我们这儿时事件可能已经被吃掉）。
//
// 为什么不在 `onUpdate` 里报"打字"：编辑器里还有**程序改内容**的路径（排版清理、
// 作答落章、撤销）。`onUpdate` 分不清是谁改的，而 DOM 事件分得清——没听见键声，
// 就不该把这一笔算成"作者打的"。

import { isTypingKey } from "./write-origin.ts";

export interface WriteWatchHandlers {
  /** 作者敲了键（含输入法组字） */
  typed: () => void;
  /** 整段进来（粘贴 / 拖放）：`chars` 是壳观测到的 plain text 字数 */
  pasted: (chars: number) => void;
}

/** 挂到编辑器的 DOM 上；返回解绑函数（编辑器重建时必须解绑）。 */
export function watchWriteSource(dom: HTMLElement, handlers: WriteWatchHandlers): () => void {
  const onKeyDown = (event: KeyboardEvent) => {
    if (isTypingKey(event)) handlers.typed();
  };
  const onCompositionStart = () => handlers.typed();
  const onPaste = (event: ClipboardEvent) => {
    const text = event.clipboardData?.getData("text/plain") ?? "";
    if (text.length > 0) handlers.pasted(text.length);
  };
  const onDrop = (event: DragEvent) => {
    const text = event.dataTransfer?.getData("text/plain") ?? "";
    if (text.length > 0) handlers.pasted(text.length);
  };

  // 全部走**捕获阶段**：ProseMirror 自己也在同一个元素上听这些事件，
  // 捕获阶段先拿到，就不怕它 `stopPropagation`（我们只听、不改事件）。
  dom.addEventListener("keydown", onKeyDown, true);
  dom.addEventListener("compositionstart", onCompositionStart, true);
  dom.addEventListener("paste", onPaste, true);
  dom.addEventListener("drop", onDrop, true);
  return () => {
    dom.removeEventListener("keydown", onKeyDown, true);
    dom.removeEventListener("compositionstart", onCompositionStart, true);
    dom.removeEventListener("paste", onPaste, true);
    dom.removeEventListener("drop", onDrop, true);
  };
}
