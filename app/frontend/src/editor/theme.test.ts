// 界面配色这一小片的接线验收：**算哪一套、画哪一套、什么时候跟着系统换**。
//
// 真正"存哪儿、怎么校验"在核心（`store::appearance` 有验收）；这里只盯界面这一层：
// - 库里认不出的码当没设过（回默认浅色），不被带歪；
// - "跟随系统"要把系统当前偏好问出来；系统换了深浅色，只有这一档跟着换；
// - 换一档写回偏好（写的是人家给的稳定码）；
// - 启动缓存读得回、坏值读不出来（它只是"第一次绘制"用的，不该把启动卡住）。

import { test } from "node:test";
import assert from "node:assert/strict";
import { ref } from "vue";

import type { Appearance } from "../api/core.ts";
import type { AppearanceState } from "./appearance.ts";
import {
  applyTheme,
  asThemeChoice,
  DEFAULT_THEME,
  effectiveTheme,
  readCachedTheme,
  useTheme,
  writeCachedTheme,
  type EffectiveTheme,
  type ThemeTarget,
} from "./theme.ts";

/** 偏好那一层的替身：只管 `values.theme` 与写回来的那一笔。 */
function fakeAppearance(initial: string | null = null) {
  const values = ref<Partial<Appearance> | null>(initial === null ? null : { theme: initial });
  const written: string[] = [];
  const appearance = {
    values,
    setTheme: async (value: string) => {
      written.push(value);
      values.value = { theme: value };
    },
  } as unknown as AppearanceState;
  return { appearance, values, written };
}

/** 界面那一侧的替身：记下画过什么，系统深浅色由测试说了算。 */
function fakeTarget(systemDark = false) {
  const painted: EffectiveTheme[] = [];
  let dark = systemDark;
  let listener: ((dark: boolean) => void) | null = null;
  let watching = 0;
  let stopped = 0;
  const target: ThemeTarget = {
    apply: (theme) => painted.push(theme),
    systemDark: () => dark,
    watchSystem: (onChange) => {
      watching += 1;
      listener = onChange;
      return () => {
        stopped += 1;
        listener = null;
      };
    },
  };
  return {
    target,
    painted,
    get watching() {
      return watching;
    },
    get stopped() {
      return stopped;
    },
    /** 系统换了深浅色（模拟系统偏好变化） */
    systemSwitchesTo(next: boolean) {
      dark = next;
      listener?.(next);
    },
  };
}

test("库里认不出的码 / 没设过：都当默认浅色（与核心默认一致）", () => {
  assert.equal(DEFAULT_THEME, "light");
  assert.equal(asThemeChoice(null), "light");
  assert.equal(asThemeChoice({}), "light");
  assert.equal(asThemeChoice({ theme: "sepia" }), "light");
  assert.equal(asThemeChoice({ theme: "dark" }), "dark");
  assert.equal(asThemeChoice({ theme: "system" }), "system");
});

test("画哪一套：只有「跟随系统」要问系统，另两档就是它自己", () => {
  assert.equal(effectiveTheme("light", true), "light", "作者选浅色，系统再深也不动");
  assert.equal(effectiveTheme("dark", false), "dark", "作者选深色，系统再浅也不动");
  assert.equal(effectiveTheme("system", true), "dark");
  assert.equal(effectiveTheme("system", false), "light");
});

test("画到 <html> 上：写的那个属性就是 CSS 两套色板认的", () => {
  const root = { dataset: {} as Record<string, string | undefined> };
  applyTheme(root, "dark");
  assert.equal(root.dataset.theme, "dark");
});

test("启动缓存：读得回、坏值读不出来、存储被禁也不炸", () => {
  const store = new Map<string, string>();
  const storage = {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => void store.set(key, value),
  } as unknown as Storage;

  assert.equal(readCachedTheme(storage), null, "第一次启动没有缓存");
  writeCachedTheme("dark", storage);
  assert.equal(readCachedTheme(storage), "dark");

  const broken = { getItem: () => "sepia", setItem: () => undefined } as unknown as Storage;
  assert.equal(readCachedTheme(broken), null, "坏值当没有");

  const denied = {
    getItem: () => {
      throw new Error("存储被禁");
    },
    setItem: () => {
      throw new Error("存储被禁");
    },
  } as unknown as Storage;
  assert.equal(readCachedTheme(denied), null, "读不到就算了，不该把启动卡住");
  writeCachedTheme("light", denied); // 存不下也不该抛
});

test("偏好读回来就画上；改成深色立刻换、并写回偏好", () => {
  const { appearance, written } = fakeAppearance(null);
  const ui = fakeTarget();
  const theme = useTheme(appearance, ui.target);

  assert.equal(theme.choice.value, "light", "还没读回来就是默认浅色");
  assert.deepEqual(ui.painted, ["light"], "先按默认画一套（不等读库）");

  // 库里那份读回来了：立刻换成深色
  appearance.values.value = { theme: "dark" };
  assert.equal(theme.effective.value, "dark");
  assert.deepEqual(ui.painted, ["light", "dark"]);

  // 设置面板里换一档：写回偏好（写的是稳定码），界面跟着变
  theme.pick("system");
  assert.deepEqual(written, ["system"]);
  assert.equal(theme.effective.value, "light", "系统现在是浅色");
});

test("跟随系统：系统换了深浅色跟着换；自己选的档不跟着换", () => {
  const { appearance } = fakeAppearance("system");
  const ui = fakeTarget(false);
  const theme = useTheme(appearance, ui.target);
  assert.equal(theme.effective.value, "light");
  assert.equal(ui.watching, 1, "盯上了系统深浅色");

  ui.systemSwitchesTo(true);
  assert.equal(theme.effective.value, "dark", "系统转深色，界面跟着转");
  ui.systemSwitchesTo(false);
  assert.equal(theme.effective.value, "light");

  // 自己选了浅色：系统再怎么换都不动它
  appearance.values.value = { theme: "light" };
  ui.systemSwitchesTo(true);
  assert.equal(theme.effective.value, "light", "作者自己定的那一档，系统不许改");
});
