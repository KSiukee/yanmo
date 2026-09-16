// 设定卡这一片的纯逻辑验收：拆别称、读设定、表单 ↔ 卡片、筛选项。
//
// 这一层没有 DOM，也不需要后端——它只管"摆什么 / 怎么解析"。

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  blankDraft,
  cardHeadline,
  draftOf,
  draftReady,
  formOf,
  formatAliases,
  formatAttributes,
  parseAliases,
  parseAttributes,
  visibleCards,
} from "./entity.ts";
import type { EntityCard } from "../api/entity.ts";

function card(id: number, kind: EntityCard["kind"], name: string, aliases: string[] = []): EntityCard {
  return {
    id,
    work_id: 1,
    kind,
    name,
    aliases,
    attributes: [],
    note: "",
    created_at: 0,
    updated_at: 0,
  };
}

test("别称那一栏：逗号、顿号、空格、换行都认，空段丢掉", () => {
  assert.deepEqual(parseAliases("阿文、陆大人"), ["阿文", "陆大人"]);
  assert.deepEqual(parseAliases("阿文, 陆大人；小陆"), ["阿文", "陆大人", "小陆"]);
  assert.deepEqual(parseAliases("阿文  陆大人\n小陆"), ["阿文", "陆大人", "小陆"]);
  assert.deepEqual(parseAliases("阿文、、陆大人"), ["阿文", "陆大人"], "空段不产生空别称");
  assert.deepEqual(parseAliases("   "), []);
  assert.equal(formatAliases(["阿文", "陆大人"]), "阿文、陆大人");
  assert.deepEqual(parseAliases(formatAliases(["阿文", "陆大人"])), ["阿文", "陆大人"]);
});

test("设定那一栏：一行一项，等号与中文冒号都认", () => {
  assert.deepEqual(parseAttributes("发色 = 黑"), [{ key: "发色", value: "黑" }]);
  assert.deepEqual(parseAttributes("发色：黑\n佩剑: 青霜"), [
    { key: "发色", value: "黑" },
    { key: "佩剑", value: "青霜" },
  ]);
  // 没写分隔符的整行当"只有键"（那是还没填，留着）
  assert.deepEqual(parseAttributes("师承"), [{ key: "师承", value: "" }]);
  // 整条空的丢掉；空行跳过
  assert.deepEqual(parseAttributes("\n   \n"), []);
  // 键与值都会修剪
  assert.deepEqual(parseAttributes("  发色  =   黑  "), [{ key: "发色", value: "黑" }]);
  assert.equal(formatAttributes([{ key: "发色", value: "黑" }]), "发色 = 黑");
});

test("表单 ↔ 卡片：摊开再收起来是同一张（名字与备注修剪）", () => {
  const source = card(3, "person", "陆文", ["阿文", "陆大人"]);
  source.attributes = [{ key: "发色", value: "黑" }];
  source.note = "前朝旧臣";

  const draft = draftOf(source);
  assert.equal(draft.id, 3);
  assert.equal(draft.aliasesText, "阿文、陆大人");
  assert.equal(draft.attributesText, "发色 = 黑");

  const form = formOf({ ...draft, name: "  陆文  ", note: "  前朝旧臣  " });
  assert.deepEqual(form, {
    kind: "person",
    name: "陆文",
    aliases: ["阿文", "陆大人"],
    attributes: [{ key: "发色", value: "黑" }],
    note: "前朝旧臣",
  });
});

test("名字空着不让交（名字是身份）", () => {
  assert.equal(draftReady(blankDraft()), false);
  assert.equal(draftReady({ ...blankDraft(), name: "   " }), false);
  assert.equal(draftReady({ ...blankDraft(), name: " 陆文 " }), true);
});

test("筛选：页签就是筛子——按类型挑，all 原样", () => {
  const cards = [card(1, "person", "陆文"), card(2, "setting", "藏书阁"), card(3, "person", "林昭")];
  assert.equal(visibleCards(cards, "all").length, 3);
  assert.deepEqual(visibleCards(cards, "person").map((item) => item.id), [1, 3]);
  assert.deepEqual(visibleCards(cards, "setting").map((item) => item.id), [2]);
});

test("列表那一行：名字 +（有的话）别称，别称只摆前两个", () => {
  const withAliases = card(1, "person", "陆文", ["阿文", "陆大人", "小陆"]);
  assert.equal(cardHeadline(withAliases), "陆文（阿文、陆大人）");
  // 设定不摆别称：名字是什么就显示什么
  assert.equal(cardHeadline(card(2, "setting", "藏书阁")), "藏书阁");
});
