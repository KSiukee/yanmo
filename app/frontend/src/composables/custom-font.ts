// 自定义字体文件：把壳给的地址变成一段 `@font-face`（**只在这一处注入**）。
//
// 三条分寸：
// - 字体文件不进安装包，也不由界面去读——壳把它拷进数据目录，这里只拿一个 URL；
// - 换了预设 / 系统字体，或者文件被删了，这段样式就整段撤掉（不留一个指向 404 的 @font-face）；
// - 地址读不到（文件被删 / 换机器）就安静回退到字体栈，不弹错。
import { watch } from "vue";
import type { Ref } from "vue";

import { fontUrl } from "../api/core.ts";
import { fontFileName, fontStackOf } from "../editor/fonts.ts";

const STYLE_ID = "ym-custom-font";

/**
 * 字体生效范围：`all` 时把整套界面字体换成选的那一款（写在 `<body>` 的行内变量上，
 * 盖过样式表里那份默认）；`body` 时撤掉，界面回到原来的系统字体栈。
 */
export function useFontScope(fontValue: Ref<string>, scope: Ref<string>): void {
  watch(
    [fontValue, scope],
    ([font, current]) => {
      const body = document.body;
      if (current === "all") {
        body.style.setProperty("--ym-font-ui", fontStackOf(font));
      } else {
        body.style.removeProperty("--ym-font-ui");
      }
    },
    { immediate: true },
  );
}

export function useCustomFont(fontValue: Ref<string>): void {
  watch(
    fontValue,
    async (value) => {
      const name = fontFileName(value);
      const existing = document.getElementById(STYLE_ID);
      if (name === null) {
        existing?.remove();
        return;
      }
      try {
        const url = await fontUrl(name);
        document.getElementById(STYLE_ID)?.remove();
        if (url === null) return;
        const style = document.createElement("style");
        style.id = STYLE_ID;
        style.textContent = `@font-face{font-family:"YanmoCustomFont";src:url("${url}");font-display:swap}`;
        document.head.appendChild(style);
      } catch {
        existing?.remove();
      }
    },
    { immediate: true },
  );
}
