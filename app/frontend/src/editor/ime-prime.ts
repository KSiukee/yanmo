// i18n-allow-file: 本文件产出的是**诊断日志条目**（给排查看的时间线），不是界面文案；
// 界面文案仍然只在 `src/locales/`。
//
// 输入法预热：把 WebView2 的输入法（TSF）先"引"到这个网页上，再交焦点给正文。
//
// # 为什么需要这个（不是猜测，是踩出来的）
//
// WebView2 在 `contentEditable` 上的输入法集成有**上游未修缺陷**：第一次把焦点交给正文时，
// 输入法可能根本没挂上来——打拼音没有任何反应，必须切一次输入语言再切回来才好；
// 而同一页面里的普通 `<input>` / `<textarea>` 一直是正常的。
//
// 真机日志把这条钉死了：窗口有焦点、正文也拿到了 DOM 焦点，但**组字事件一个都不来**；
// 切一次语言之后组字事件立刻到齐，此后焦点进进出出都正常。
//
// 绕法：先在页面上放一个**真实但看不见的输入框**并聚焦它——输入法会随它挂到 WebView 上；
// 片刻后把焦点交给正文，输入法就已经在岗了。整个动作几十毫秒，作者看不到（输入框在屏幕外）。

/** 预热用的输入框（只在预热那一小会儿存在）。 */
const PRIME_ID = "yanmo-ime-prime";

/**
 * 预热输入法，然后把焦点交出去。
 *
 * - 输入框放在屏幕外、1×1、`aria-hidden`、`tabindex="-1"`：不进阅读顺序，也看不见；
 * - 写完就删（不留在 DOM 里当垃圾）；
 * - `then` 在预热之后执行——调用方把"真正要把焦点交给谁"放进去。
 */
export function primeImeThen(then: () => void, onNote?: (text: string) => void): void {
  const box = document.createElement("input");
  box.id = PRIME_ID;
  box.type = "text";
  box.tabIndex = -1;
  box.setAttribute("aria-hidden", "true");
  box.autocomplete = "off";
  box.style.cssText =
    "position:fixed;top:-100px;left:0;width:1px;height:1px;opacity:0.01;border:0;padding:0;";

  const cleanup = () => {
    if (box.parentNode) box.parentNode.removeChild(box);
  };

  try {
    document.body.appendChild(box);
    box.focus();
    onNote?.(`输入法预热：先聚焦隐藏输入框（焦点=${document.activeElement?.tagName.toLowerCase() ?? "?"}）`);
  } catch {
    // 预热失败不该拦住启动：直接交焦点
    cleanup();
    then();
    return;
  }

  // 给输入法一点时间挂上（同步连着换焦点，系统来不及重新问一遍输入法是谁）
  window.setTimeout(() => {
    then();
    onNote?.(`输入法预热：已把焦点交给正文（焦点=${document.activeElement?.tagName.toLowerCase() ?? "?"}）`);
    window.setTimeout(cleanup, 500);
  }, 60);
}
