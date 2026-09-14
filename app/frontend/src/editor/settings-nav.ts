// 设置面板的**分区表**：左边一列分类，右边只显示当前这一类。
//
// 为什么把这张表单独放（纯数据 + 纯函数）：
// - 设置项只会越加越多，摊成一长列就是"看着乱"的根源——**分类是结构**，不该散在模板里；
// - 结构要能被测试盯着：id 不许重复、文案键必须在字典里（加一类忘一处，会静默变成空格子，
//   而"点了没反应"是最难查的一类界面毛病）；
// - 与渲染分开之后，改分类只动这张表，不动组件。

/** 一个分类。 */
export interface SettingsSection {
  /** 稳定 id：选中态、测试、（将来）"上次看的是哪一类"都用它 */
  id: string;
  /** 分类名（界面字典里的键） */
  labelKey: string;
}

/**
 * 分类的顺序 = 界面上的顺序。
 *
 * 排法跟着"作者多久来一次"走：**写的时候想调**的在前，**装了才看一眼**的在后。
 */
export const SETTINGS_SECTIONS: readonly SettingsSection[] = [
  { id: "writing", labelKey: "settings.nav_writing" },
  { id: "typography", labelKey: "settings.nav_typography" },
  { id: "naming", labelKey: "settings.nav_naming" },
  { id: "location", labelKey: "settings.nav_location" },
  { id: "about", labelKey: "settings.nav_about" },
];

/** 打开设置时先看哪一类（第一类；表空了给空串，调用方不必判空）。 */
export function firstSection(): string {
  return SETTINGS_SECTIONS[0]?.id ?? "";
}

/** 这一类在表里吗（防止把选中的 id 写错而整块面板空白）。 */
export function hasSection(id: string): boolean {
  return SETTINGS_SECTIONS.some((section) => section.id === id);
}
