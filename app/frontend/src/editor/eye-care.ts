// 护眼滤色：**整窗一层暖黄的膜**，幅度可调（0~100）。
//
// 三件事分清楚，别搅在一起（与 `theme.ts` 同一条写法）：
// 1. **作者调的幅度**——存在核心（`appearance.eye_care`，跟人不跟书、**只走全局**）；
// 2. **这一刻那层膜该多深**——纯函数：幅度往里、当前配色往里，出一个 0~1 的数；
// 3. **怎么落到屏幕上**——整窗一个 `<div class="ym-eye-care">`，颜色写死在 style.css 的
//    两套色板里（颜色字面量只许住那儿，有机械守卫盯着），这里只给它一个不透明度。
//
// ★ 幅度 0 ＝ **连那个元素都不存在**：不是"画一层全透明的"，而是一个像素都不碰。
//   `paint(0)` 的那条路上不做任何 DOM 写入。
//
// ★ 深浅要**合计**一个上限：深色底是暖黑，再压一层强黄会把两头一起吃掉
//   （浅字发灰、黑底发黄，糊成一团）。所以深色那一档整条曲线按 `DARK_SCALE` 收窄——
//   收窄而不是"到某个数就封顶"：封顶会让滑杆右半截推着没反应，看着像坏了。
//
// 边界：**只染研墨自己的窗口**。整机滤色不做（改系统设置 / 往别的进程注入，两条都不走，
// 见 `model::eye_care` 的文件头）；想要整机滤色的作者，系统自带的夜间模式就是他要的东西
// ——设置面板里写着这句话。
//
// 启动那一下：那层膜会晚一个来回（幅度要从核心读回来）。**不另存一份启动缓存**：
// 与配色的白闪不是一回事（那个是整屏发白，这个只是晚一点变色），多存一份就多一份会腐烂
// 的第二真相。

import { computed, watch, type ComputedRef, type Ref } from "vue";

import type { Appearance } from "../api/core.ts";
import type { AppearanceState } from "./appearance.ts";
import type { EffectiveTheme } from "./theme.ts";

/** 幅度上限（百分比）——与核心 `model::eye_care::MAX_EYE_CARE` 同口径。 */
export const MAX_EYE_CARE = 100;

/** 默认幅度：**关**（0）——不给没提过这件事的人换底子（核心那边也是这个默认）。 */
export const DEFAULT_EYE_CARE = 0;

/** 浅色底上、幅度拉满时那层膜的不透明度（深浅两档都从它出发，深色再收窄）。 */
export const FULL_VEIL_OPACITY = 0.4;

/**
 * 深色底上的收窄系数（**合计上限**就落在这一处）。
 *
 * 为什么是 0.6 而不是更猛的一刀：0.6 让深色那一档最深只到浅色的六成，明暗两头都还留着
 * 台阶；再往上压，黑底会被一层黄托亮、浅字跟着发灰，两头一起糊。
 */
export const DARK_SCALE = 0.6;

/** 到这儿就说一句"字可能不好认了"——**只提示，不挡人**（与背景图那条同一个分寸）。 */
export const EYE_CARE_HINT = 70;

/** 那层膜的元素类名（样式在 `style.css`：形状在这儿认，颜色在那儿住）。 */
export const VEIL_CLASS = "ym-eye-care";

/** 那层膜的不透明度挂在哪个自定义属性上（`style.css` 的 `.ym-eye-care` 读它）。 */
export const VEIL_OPACITY_VAR = "--ym-eye-care-strength";

/** 库里那份（可能是 null / 缺项 / 坏值）→ 能用的一档（0~100 的整数）。 */
export function asEyeCare(values: Partial<Appearance> | null | undefined): number {
  const raw = values?.eye_care;
  if (typeof raw !== "number" || !Number.isFinite(raw)) return DEFAULT_EYE_CARE;
  return Math.min(MAX_EYE_CARE, Math.max(0, Math.round(raw)));
}

/** 到"字可能不好认了"那一档没有（面板据此决定说不说那句话）。 */
export function eyeCareIsStrong(values: Partial<Appearance> | null | undefined): boolean {
  return asEyeCare(values) >= EYE_CARE_HINT;
}

/**
 * 这一刻那层膜该多深：**0~1**（0 ＝ 一个像素都不染）。
 *
 * 两件事一起算：幅度占满幅的多少、以及压在什么底上（深色收窄，见 `DARK_SCALE`）。
 * 纯函数：给同一对输入永远给同一个数，所以深浅合计那条上限可以直接单测。
 */
export function eyeCareStrength(intensity: number, theme: EffectiveTheme): number {
  const level = Math.min(MAX_EYE_CARE, Math.max(0, Math.round(intensity)));
  if (level <= 0) return 0; // ★ 关：连一小层都不铺
  const veil = (level / MAX_EYE_CARE) * FULL_VEIL_OPACITY;
  const scaled = theme === "dark" ? veil * DARK_SCALE : veil;
  return Math.round(scaled * 1000) / 1000;
}

/** 界面这一侧要跟外面打的那个交道（浏览器里就是往 `<body>` 上挂一层；测试给替身）。 */
export interface EyeCareTarget {
  /**
   * 铺上这一层：给一个 **0~1** 的不透明度。
   *
   * `0` ＝ **把那一层整个撤掉**（不是画一层透明的）——"0 就是关"这条判据落在这里，
   * 也是这个接口唯一一件"必须做到"的事。
   */
  paint(strength: number): void;
}

/** 真机上的那一个：整窗挂一层 `<div>`，颜色与形状归样式表。 */
export function browserEyeCareTarget(doc: Document = globalThis.document): EyeCareTarget {
  const find = () => doc.querySelector<HTMLElement>(`.${VEIL_CLASS}`);
  return {
    paint: (strength) => {
      const existing = find();
      if (strength <= 0) {
        // 关：撤掉那一层（一次都不创建）。**不写一行 DOM**才是"一个像素都不染"。
        existing?.remove();
        return;
      }
      const veil = existing ?? doc.createElement("div");
      if (!existing) {
        veil.className = VEIL_CLASS;
        // 纯装饰的一层膜：读屏软件不该念它，鼠标也不该被它挡住（样式表里还有 pointer-events）
        veil.setAttribute("aria-hidden", "true");
        doc.body.appendChild(veil);
      }
      veil.style.setProperty(VEIL_OPACITY_VAR, String(strength));
    },
  };
}

export interface EyeCareState {
  /** 作者调的幅度（0~100；读不到就是默认关） */
  intensity: ComputedRef<number>;
  /** 这一刻那层膜多深（0~1；0 ＝ 按上面那条规矩撤掉） */
  strength: ComputedRef<number>;
  /** 拉滑杆：写库（回读后这里跟着变） */
  set: (value: number) => void;
}

/**
 * 把护眼滤色接起来：**读回来就铺上、改了立刻换、换配色时深浅合计重算**。
 *
 * `flush: "sync"`：幅度或配色一变那层膜就跟着变，不留"晚一拍"的空窗
 * （与 `useTheme` 同一条分寸，于是这件事能直接单测，不必等一次 tick）。
 */
export function useEyeCare(
  appearance: AppearanceState,
  theme: Ref<EffectiveTheme> | ComputedRef<EffectiveTheme>,
  target: EyeCareTarget = browserEyeCareTarget(),
): EyeCareState {
  const intensity = computed(() => asEyeCare(appearance.values.value));
  const strength = computed(() => eyeCareStrength(intensity.value, theme.value));

  watch(strength, (next) => target.paint(next), { immediate: true, flush: "sync" });

  return {
    intensity,
    strength,
    set: (value) => void appearance.setEyeCare(value),
  };
}
