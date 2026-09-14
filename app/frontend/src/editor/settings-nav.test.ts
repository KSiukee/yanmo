// 设置面板分区表的验收：**结构本身要有人看着**。
//
// 分类是数据，数据写错了界面不会报错，只会"点了没反应"或"露出一格空白"——
// 那是最难查的一类毛病，所以让机器盯着。

import { test } from "node:test";
import assert from "node:assert/strict";

import { has, t } from "../locales/index.ts";
import { SETTINGS_SECTIONS, firstSection, hasSection } from "./settings-nav.ts";

test("分类表不是空的，且 id 不重复", () => {
  assert.ok(SETTINGS_SECTIONS.length >= 2, "分区的意义就在于不止一类");
  const ids = SETTINGS_SECTIONS.map((section) => section.id);
  assert.equal(new Set(ids).size, ids.length, `id 重复了：${ids.join("、")}`);
  for (const id of ids) {
    assert.ok(id.trim() !== "", "id 不许是空串");
  }
});

test("每一类的名字都在界面字典里（否则导航上会露出一串键名）", () => {
  const missing = SETTINGS_SECTIONS.filter((section) => !has(section.labelKey));
  assert.deepEqual(
    missing.map((section) => section.labelKey),
    [],
    "这些键不在字典里——加分类时忘了加文案，界面上会是 `settings.nav_xxx` 这种键名",
  );
  const empty = SETTINGS_SECTIONS.filter((section) => t(section.labelKey).trim() === "");
  assert.deepEqual(empty, [], "分类名字不许是空的");
});

test("打开设置落在第一类上，且它确实在表里", () => {
  const first = firstSection();
  assert.equal(first, SETTINGS_SECTIONS[0].id);
  assert.ok(hasSection(first), "firstSection() 给出来的 id 必须能在表里找到");
});

test("认不出来的 id 一律说不认识（别让界面去猜）", () => {
  assert.equal(hasSection("nope"), false);
  assert.equal(hasSection(""), false);
});
