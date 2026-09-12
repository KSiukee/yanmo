// 界面文案集中的**唯一一处**：组件与编辑器逻辑都从这里取字。
//
// 三条纪律：
// 1. **只搬家，不翻译**——现在只有简体中文一种，值就是原来散在各文件里的句子，一字未改；
// 2. **查不到就原样返回键**——宁可屏幕上出现一个 `shelf.empty`（一眼看得见、改起来快），
//    也不要静默变成空白，那种毛病最难查；
// 3. **先不引 vue-i18n**——只有一种语言时引库是过度设计。键的形状（点分小写）与
//    vue-i18n 兼容，将来真要换过去，「搬家」这一步已经做完了，剩下的是加字典。
//
// 语言码用 `zh-Hans` 而不是 `zh`：为将来简繁分开（`zh-Hant`）留位。
import zhHans from "./zh-Hans.json" with { type: "json" };

/** 当前界面语言（语言切换界面后补；见设置面板那条任务）。 */
export const DEFAULT_LOCALE = "zh-Hans";

const DICTS: Record<string, Record<string, string>> = { "zh-Hans": zhHans };

/** 取一句界面文案，`{名字}` 用 params 填。 */
export function t(key: string, params?: Record<string, string | number>): string {
  return fill(DICTS[DEFAULT_LOCALE][key] ?? key, params);
}

/** 把 `{名字}` 换成取值；参数里没有的名字原样留着（不静默吞掉）。 */
export function fill(template: string, params?: Record<string, string | number>): string {
  if (!params) {
    return template;
  }
  return template.replace(/\{(\w+)\}/g, (whole, name: string) =>
    name in params ? String(params[name]) : whole,
  );
}

/** 字典里有没有这一条（守卫与测试用）。 */
export function has(key: string): boolean {
  return key in DICTS[DEFAULT_LOCALE];
}

/** 当前语言的全部键（穷举对表用）。 */
export function keys(): string[] {
  return Object.keys(DICTS[DEFAULT_LOCALE]);
}
