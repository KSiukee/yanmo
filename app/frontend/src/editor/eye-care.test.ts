// 护眼滤色这一小片的接线验收：**算多深、铺不铺、深浅怎么合计**。
//
// 真正"存哪儿、怎么校验"在核心（`tests/eye_care.rs` 有验收）；这里只盯界面这一层：
// - 库里认不出的 / 越界的值都当没设过或夹住（回默认：关）；
// - ★ 幅度 0 ＝ **一个像素都不染**（连那一层元素都不该存在）；
// - ★ 深色底上整条曲线收窄（合计上限），且 0 永远是不染；
// - 换配色 / 改幅度都不用等一次 tick，当场生效；
// - 真机那一层挂上去之后带什么类名与不透明度（类名与样式表对不上＝功能静默失效）。

import { test } from "node:test";
import assert from "node:assert/strict";
import { ref } from "vue";

import type { Appearance } from "../api/core.ts";
import type { AppearanceState } from "./appearance.ts";
import type { EffectiveTheme } from "./theme.ts";
import {
  asEyeCare,
  browserEyeCareTarget,
  DARK_SCALE,
  DEFAULT_EYE_CARE,
  eyeCareIsStrong,
  eyeCareStrength,
  FULL_VEIL_OPACITY,
  MAX_EYE_CARE,
  useEyeCare,
  VEIL_CLASS,
  VEIL_OPACITY_VAR,
  type EyeCareTarget,
} from "./eye-care.ts";

/** 偏好那一层的替身：只管 `values.eye_care` 与写回来的那一笔。 */
function fakeAppearance(initial: number | null = null) {
  const values = ref<Partial<Appearance> | null>(initial === null ? null : { eye_care: initial });
  const written: number[] = [];
  const appearance = {
    values,
    setEyeCare: async (value: number) => {
      written.push(value);
      values.value = { eye_care: value };
    },
  } as unknown as AppearanceState;
  return { appearance, values, written };
}

/** 屏幕那一侧的替身：记下每一次"铺多深"。 */
function fakeTarget() {
  const painted: number[] = [];
  const target: EyeCareTarget = { paint: (strength) => painted.push(strength) };
  return { target, painted };
}

/** 一个够用的假文档：只实现这一层用到的那几件（createElement / querySelector / body）。 */
function fakeDom() {
  let attached: any = null;
  const created: any[] = [];
  const doc = {
    createElement: () => {
      const el: any = {
        className: "",
        attrs: {} as Record<string, string>,
        styles: {} as Record<string, string>,
        removed: false,
        setAttribute: (key: string, value: string) => {
          el.attrs[key] = value;
        },
        style: {
          setProperty: (key: string, value: string) => {
            el.styles[key] = value;
          },
        },
        remove: () => {
          el.removed = true;
          if (attached === el) attached = null;
        },
      };
      created.push(el);
      return el;
    },
    querySelector: () => attached,
    body: {
      appendChild: (el: any) => {
        attached = el;
      },
    },
  };
  return {
    doc: doc as unknown as Document,
    created,
    get attached() {
      return attached;
    },
  };
}

test("库里的坏值 / 越界值：回默认关或夹进范围（界面不许自己编一个数）", () => {
  assert.equal(DEFAULT_EYE_CARE, 0, "默认关：没提过这件事的人不该看到一层黄");
  assert.equal(asEyeCare(null), 0);
  assert.equal(asEyeCare({}), 0);
  assert.equal(asEyeCare({ eye_care: -20 }), 0, "负数（＝清掉那一层）当关");
  assert.equal(asEyeCare({ eye_care: 55 }), 55);
  assert.equal(asEyeCare({ eye_care: 55.6 }), 56, "滑杆给的是整数，读数也取整");
  assert.equal(asEyeCare({ eye_care: 500 }), MAX_EYE_CARE, "越界夹住——纯观感，不值得报错");
});

test("★ 幅度 0：深浅两档都一个像素都不染", () => {
  assert.equal(eyeCareStrength(0, "light"), 0);
  assert.equal(eyeCareStrength(0, "dark"), 0);
  assert.equal(eyeCareStrength(-30, "dark"), 0, "坏值也当关");
});

test("★ 深浅合计上限：深色那一档整条曲线收窄，且最高不超过浅色的六成", () => {
  assert.equal(eyeCareStrength(MAX_EYE_CARE, "light"), FULL_VEIL_OPACITY);
  assert.equal(eyeCareStrength(MAX_EYE_CARE, "dark"), FULL_VEIL_OPACITY * DARK_SCALE);

  // 逐档都比浅色浅，而且滑杆往右推不会突然不涨（收窄不是封顶）
  let previous = 0;
  for (let level = 0; level <= MAX_EYE_CARE; level += 10) {
    const light = eyeCareStrength(level, "light");
    const dark = eyeCareStrength(level, "dark");
    assert.ok(dark <= light, `${level}%：深色不许比浅色还深`);
    assert.ok(light >= previous, "浅色：幅度越深那层膜越深");
    assert.ok(level === 0 || light > previous, `${level}%：往右推要看得见变化（不是封顶）`);
    previous = light;
  }
  assert.ok(
    eyeCareStrength(100, "dark") < eyeCareStrength(100, "light"),
    "深色拉满要比浅色浅——不然黑底配强黄，两头一起糊",
  );
});

test("到「字可能不好认了」那一档：只说一句，不挡人", () => {
  assert.equal(eyeCareIsStrong({ eye_care: 69 }), false);
  assert.equal(eyeCareIsStrong({ eye_care: 70 }), true);
});

test("★ 读回来就铺上：没设过就是关（paint 0 = 撤掉那一层）", () => {
  const { appearance } = fakeAppearance(null);
  const theme = ref<EffectiveTheme>("light");
  const ui = fakeTarget();
  const eye = useEyeCare(appearance, theme, ui.target);

  assert.equal(eye.intensity.value, 0);
  assert.deepEqual(ui.painted, [0], "还没读回偏好：先按「不染」处理");

  // 库里那份读回来了：当场铺上（不用等一次 tick）
  appearance.values.value = { eye_care: 50 };
  assert.equal(eye.intensity.value, 50);
  assert.deepEqual(ui.painted, [0, 0.2]);

  // 拉滑杆：写回偏好（写的是那个数），界面跟着变
  eye.set(80);
  assert.deepEqual(ui.painted, [0, 0.2, 0.32]);
});

test("★ 换配色：深浅合计当场重算（同一份偏好，深色底下更浅）", () => {
  const { appearance } = fakeAppearance(100);
  const theme = ref<EffectiveTheme>("light");
  const ui = fakeTarget();
  const eye = useEyeCare(appearance, theme, ui.target);

  assert.equal(eye.strength.value, FULL_VEIL_OPACITY);

  theme.value = "dark";
  assert.equal(eye.strength.value, FULL_VEIL_OPACITY * DARK_SCALE);
  assert.deepEqual(ui.painted, [FULL_VEIL_OPACITY, FULL_VEIL_OPACITY * DARK_SCALE]);

  // 关掉：撤掉那一层（连透明的都不铺）
  eye.set(0);
  assert.equal(ui.painted.at(-1), 0, "关掉那一层：铺的是 0（＝撤掉）");
});

test("真机那一层：0 不创建任何元素；开着才挂一层，关掉就撤掉", () => {
  const dom = fakeDom();
  const target = browserEyeCareTarget(dom.doc);

  target.paint(0);
  assert.equal(dom.created.length, 0, "★ 幅度 0 ＝ 一个像素都不染：连元素都不许创建");
  assert.equal(dom.attached, null);

  target.paint(0.3);
  const veil = dom.attached;
  assert.ok(veil, "开着就该挂上那一层");
  assert.equal(veil.className, VEIL_CLASS, "类名要与样式表里那一条对得上");
  assert.equal(veil.attrs["aria-hidden"], "true", "纯装饰的一层膜，读屏软件不该念它");
  assert.equal(veil.styles[VEIL_OPACITY_VAR], "0.3", "颜色住样式表，这里只给不透明度");
  assert.equal(dom.created.length, 1);

  // 再调幅度：还是那一层，不重复挂
  target.paint(0.2);
  assert.equal(dom.created.length, 1, "不该一层层往上叠");
  assert.equal(dom.attached.styles[VEIL_OPACITY_VAR], "0.2");

  // 关掉：撤掉
  target.paint(0);
  assert.equal(veil.removed, true);
  assert.equal(dom.attached, null);
  assert.equal(dom.created.length, 1, "撤掉不等于新建一个");
});
