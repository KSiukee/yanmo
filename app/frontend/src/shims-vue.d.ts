// 让纯 `tsc` 也能认 `.vue` 单文件组件（不装 vue-tsc 也能做类型检查）。
// 只声明"能 import"，组件内部类型不在这一步校验——那需要 vue-tsc。
declare module "*.vue" {
  import type { DefineComponent } from "vue";
  const component: DefineComponent<Record<string, unknown>, Record<string, unknown>, unknown>;
  export default component;
}
