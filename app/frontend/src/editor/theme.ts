// 界面配色：**浅色 / 深色 / 跟随系统**。
//
// 三件事分清楚，别搅在一起：
// 1. **作者选的是哪一档**——存在核心（`appearance.theme`，跟人不跟书，只走全局）；
// 2. **现在这一刻该画哪一套**——"跟随系统"要把系统的深浅色问出来，这一步是**纯函数**，好测；
// 3. **怎么落到界面上**——给 `<html>` 打一个 `data-theme`，两套色板各自认一个值。
//
// 为什么走 `data-theme` 而不是往元素上写内联样式：色板是 CSS 的事
// （`[data-theme="dark"]` 覆盖那一组变量），界面只负责说"现在哪一套"——
// 加一个新的色板变量不用碰任何组件。
//
// ⚠️ `:root` 里 `color-scheme` 写的是 `light`（**不是** `light dark`），见 style.css 那段：
// 写了 `light dark`，系统一开深色，WebView 就把**默认滚动条**画成深色（浅色稿纸配黑滚动条，
// 真报过）。"跟随系统"是这里**自己算**出来的，不交给浏览器的 `light dark` 去猜。
//
// 为什么还留一份启动缓存（localStorage）：见 `readCachedTheme` 那段——**省一次白闪**。
//
// ⚠️ 已经加上了一层「再叠一层色」的开关（护眼滤色：整窗黄底、幅度可调）——它**不改这套变量**：
// 它是在外面另挂一层（`editor/eye-care.ts` + style.css 的 `.ym-eye-care`），而且幅度要与深色
// **合计**设个上限（深色配强黄滤色会把对比度两头一起吃掉，糊成一团）。那一层就是读这里的
// `effective` 才知道自己压在什么底上的。

import { computed, getCurrentScope, onScopeDispose, ref, watch, type ComputedRef, type Ref } from "vue";

import type { Appearance } from "../api/core";
import type { AppearanceState } from "./appearance";

/** 作者选的那一档（稳定码与核心一致）。 */
export type ThemeChoice = "light" | "dark" | "system";

/** 此刻实际画在屏幕上的那一套（"跟随系统"已经被系统当前偏好落定）。 */
export type EffectiveTheme = "light" | "dark";

/** 三档（设置面板上的顺序；穷举测试拿它对表）。 */
export const THEME_CHOICES: readonly ThemeChoice[] = ["light", "dark", "system"];

/** 默认那一档：**浅色**（与研墨一直以来的样子一致；核心那边也是这个默认）。 */
export const DEFAULT_THEME: ThemeChoice = "light";

/** 库里那份（可能是 null / 缺项 / 不认识的码）→ 能用的一档。 */
export function asThemeChoice(values: Partial<Appearance> | null | undefined): ThemeChoice {
  const raw = values?.theme;
  return raw === "dark" || raw === "system" ? raw : DEFAULT_THEME;
}

/** 落到屏幕上的是哪一套：**只有"跟随系统"需要问系统**，另两档就是它自己。 */
export function effectiveTheme(choice: ThemeChoice, systemPrefersDark: boolean): EffectiveTheme {
  if (choice === "system") return systemPrefersDark ? "dark" : "light";
  return choice;
}

/** 把"现在哪一套"写到 `<html>` 上（CSS 两套色板认的就是这个属性）。 */
export function applyTheme(root: { dataset: Record<string, string | undefined> }, theme: EffectiveTheme): void {
  root.dataset.theme = theme;
}

/** 启动缓存那一个键（只存"上次画的是哪一套"，不存作者的档）。 */
const CACHE_KEY = "ym.theme";

/**
 * 上次画的是哪一套（没有 / 读不出来就是 null）。
 *
 * 它是**给第一次绘制用的一点缓存**，不是第二份真相：作者选的档永远以核心那份为准，
 * 读回来之后 `useTheme` 立刻按核心纠正一次。留着它的唯一理由：深色用户每次启动都先
 * 闪一下白屏——"夜里码字不瞎眼"的功能，开场先闪一下白最煞风景。
 */
export function readCachedTheme(storage: Storage | undefined = globalThis.localStorage): EffectiveTheme | null {
  try {
    const raw = storage?.getItem(CACHE_KEY);
    return raw === "dark" || raw === "light" ? raw : null;
  } catch {
    return null; // 隐私模式 / 存储被禁：存不下就算了，不该因为一点缓存把启动卡住
  }
}

/** 记住这次画的是哪一套（存不下就算了：核心那份才是真相）。 */
export function writeCachedTheme(
  theme: EffectiveTheme,
  storage: Storage | undefined = globalThis.localStorage,
): void {
  try {
    storage?.setItem(CACHE_KEY, theme);
  } catch {
    // 同上：缓存失败不影响任何功能
  }
}

/** 界面这一侧要跟外面打的交道（浏览器里就是 `<html>` + `matchMedia`；测试给替身）。 */
export interface ThemeTarget {
  /** 现在画哪一套 */
  apply(theme: EffectiveTheme): void;
  /** 系统现在是不是深色 */
  systemDark(): boolean;
  /** 盯着系统深浅色的变化（返回取消订阅）——只有"跟随系统"那一档需要 */
  watchSystem(onChange: (dark: boolean) => void): () => void;
}

/** 真机上的那一个：写到 `<html data-theme>`，系统偏好问 `prefers-color-scheme`。 */
export function browserThemeTarget(): ThemeTarget {
  const query = () => globalThis.matchMedia("(prefers-color-scheme: dark)");
  return {
    apply: (theme) => applyTheme(globalThis.document.documentElement, theme),
    systemDark: () => query().matches,
    watchSystem: (onChange) => {
      const list = query();
      const handler = (event: MediaQueryListEvent) => onChange(event.matches);
      list.addEventListener("change", handler);
      return () => list.removeEventListener("change", handler);
    },
  };
}

export interface ThemeState {
  /** 此刻实际画的那一套 */
  effective: Ref<EffectiveTheme>;
  /** 作者选的那一档（读不到就是默认浅色） */
  choice: ComputedRef<ThemeChoice>;
  /** 换一档：写库（回读后这里跟着变） */
  pick: (next: ThemeChoice) => void;
}

/**
 * 把配色接起来：**读回来就画上、改了立刻换、跟随系统时系统换了自己换**。
 *
 * `flush: "sync"`：偏好一变这一套就跟着变，不留"晚一拍"的空窗——于是这件事能直接单测，
 * 不必等一次 tick（与 `useAsideTab` 同一条分寸）。
 */
export function useTheme(appearance: AppearanceState, target: ThemeTarget = browserThemeTarget()): ThemeState {
  const choice = computed(() => asThemeChoice(appearance.values.value));
  const effective = ref<EffectiveTheme>("light");

  function refresh(): void {
    const next = effectiveTheme(choice.value, target.systemDark());
    effective.value = next;
    target.apply(next);
    writeCachedTheme(next);
  }

  watch(choice, refresh, { immediate: true, flush: "sync" });

  // 系统的深浅色换了：只有"跟随系统"那一档要当场跟着换（另两档是作者自己定的，不动）
  const stop = target.watchSystem(() => {
    if (choice.value === "system") refresh();
  });
  // 组件没了就退订（不在组件作用域里调用时——比如单测——跳过这一步，Vue 会告警）
  if (getCurrentScope()) onScopeDispose(stop);

  return {
    effective,
    choice,
    pick: (next) => void appearance.setTheme(next),
  };
}
