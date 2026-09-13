// i18n-allow-file: 本文件产出的是**诊断日志的条目**（给排查看的时间线），不是界面文案；
// 界面文案仍然只在 `src/locales/`。
//
// 启动诊断：把"焦点在哪个元素上""输入法有没有真的挂上来"如实递给壳。
//
// 只在壳开了 `--diagnose` 时才有意义（没开时那条命令是空转，开销可忽略），所以这里不做开关判断——
// 少一个分支就少一处"诊断没生效"的可能。
//
// 为什么盯这两件事：那个只在部分 Win10 机器上出现的毛病，分水岭就是
// "焦点是不是真在正文里" 与 "输入法有没有发出组字事件"。没有这两样，只能靠猜。

import { diagnoseNote } from "../api/core";

const seen = new Set<string>();

/** 同一个事实只报第一次（时间线要能读，不能刷屏）。 */
function once(tag: string, detail: string): void {
  if (seen.has(tag)) return;
  seen.add(tag);
  void diagnoseNote(`${tag}${detail ? `（${detail}）` : ""}`).catch(() => {});
}

/** 正文元素长什么样——给日志一个"它是 contentEditable 吗"的底。 */
function describe(node: Element | null): string {
  if (!node) return "没有元素";
  const element = node as HTMLElement;
  const editable = element.isContentEditable ? "可编辑" : "不可编辑";
  return `${element.tagName.toLowerCase()}.${element.className || "(无类名)"} ${editable}`;
}

/**
 * 盯住编辑区：焦点进出、输入法组字、以及"窗口有没有焦点"。
 *
 * 返回清理函数（切章重建编辑器时调）。
 */
export function watchFocusAndIme(dom: HTMLElement): () => void {
  const onFocus = () => once("正文拿到焦点", describe(document.activeElement));
  const onBlur = () => once("正文失去焦点", describe(document.activeElement));
  const onCompositionStart = () => once("输入法开始组字", "");
  const onCompositionEnd = (event: CompositionEvent) => {
    void diagnoseNote(`输入法结束组字（${String(event.data ?? "").slice(0, 12)}）`).catch(() => {});
  };
  const onKeyDown = (event: KeyboardEvent) => {
    // IME 组字期间浏览器给的是 keyCode 229：它到了，说明输入法确实在用这个元素
    if (event.keyCode === 229 || event.isComposing) once("输入法按键（229/组字中）", "");
    else if (event.key === "Enter") once("普通回车（说明当前不在组字）", "");
  };

  dom.addEventListener("focus", onFocus, true);
  dom.addEventListener("blur", onBlur, true);
  dom.addEventListener("compositionstart", onCompositionStart, true);
  dom.addEventListener("compositionend", onCompositionEnd, true);
  dom.addEventListener("keydown", onKeyDown, true);

  // 开窗头几秒，每秒报一次"页面/窗口有没有焦点"——那条毛病的现场就在这几秒里
  let ticks = 0;
  const timer = window.setInterval(() => {
    ticks += 1;
    const active = describe(document.activeElement);
    void diagnoseNote(
      `第 ${ticks} 秒：文档有焦点=${document.hasFocus()} 焦点元素=${active}`,
    ).catch(() => {});
    if (ticks >= 6) window.clearInterval(timer);
  }, 1000);

  // 点进页面也算一个事实（"点了也不行"是很关键的一条）
  const onPointerDown = () => once("页面被点击", describe(document.activeElement));
  dom.addEventListener("pointerdown", onPointerDown, true);

  return () => {
    window.clearInterval(timer);
    dom.removeEventListener("focus", onFocus, true);
    dom.removeEventListener("blur", onBlur, true);
    dom.removeEventListener("compositionstart", onCompositionStart, true);
    dom.removeEventListener("compositionend", onCompositionEnd, true);
    dom.removeEventListener("keydown", onKeyDown, true);
    dom.removeEventListener("pointerdown", onPointerDown, true);
  };
}
