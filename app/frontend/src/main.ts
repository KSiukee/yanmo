import { createApp } from "vue";
import App from "./App.vue";
import "./style.css";
import { applyTheme, readCachedTheme } from "./editor/theme.ts";

// 先把**上次那套配色**画上，再挂载界面（省一次白闪）。
//
// 这一步只是"第一次绘制用的一点缓存"：作者选的档永远以核心那份为准——挂载之后
// `useTheme` 会把偏好读回来并当场纠正（读不到就回浅色）。留着它的唯一理由：
// 深色用户每次启动都先闪一下白屏，"夜里码字不瞎眼"的功能，开场先晃一下白最煞风景。
applyTheme(document.documentElement, readCachedTheme() ?? "light");

createApp(App).mount("#app");
