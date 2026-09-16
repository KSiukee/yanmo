// 设定卡对话框的**纯展示逻辑**：一串别称怎么拆、一项一行怎么读、类型怎么称呼。
//
// 单独成文件的原因与 `creator.ts` 同一条：这些都是纯函数，能单测；
// 组件只管把它们摆上去。文案一律从字典取（`entity.*`）——界面文案只有那一处来源。
//
// 两条分寸（与核心一致）：
// - **空项入口就丢**：别称里的空段、整条空的设定都是手滑；
// - **只有键没值的那条留着**（"还没填"是作者的状态）。

import { t } from "../locales/index.ts";
import type {
  Attribute,
  EntityBoard,
  EntityCard,
  EntityCardForm,
  EntityKind,
} from "../api/entity.ts";

/** 两种类型（界面上那两个筛选项的顺序）。 */
export const ENTITY_KINDS: EntityKind[] = ["person", "setting"];

/** 类型怎么称呼；字典里没有就照原样露出来（比静默说成别的强）。 */
export function kindLabel(kind: string): string {
  const key = `entity.kind.${kind}`;
  const text = t(key);
  return text === key ? kind : text;
}

/**
 * 别称那一栏的文本 → 一串别称。
 *
 * 作者会用什么分隔？中文写作里逗号、顿号、空格、换行都有人用——所以都认，
 * 空段丢掉（"阿文、、陆大人"不该产生一个空别称，那会在检测时变成假撞车）。
 */
export function parseAliases(text: string): string[] {
  return text
    .split(/[,，、;；\s]+/)
    .map((item) => item.trim())
    .filter((item) => item !== "");
}

/** 一串别称 → 那一栏的文本（分隔符从字典取：中文用顿号，别的语言换别的）。 */
export function formatAliases(aliases: string[]): string {
  return aliases.join(t("common.list_separator"));
}

/**
 * 设定那一栏的文本 → 一串键值。
 *
 * 一行一项，`键 = 值`（也认 `键：值` 与 `键: 值`——中文输入法下这三种都常见）。
 * **只有键没值的那条留着**：那是"还没填"，作者留着位置是有意的；
 * 键和值都空的行才是手滑，丢掉。
 */
export function parseAttributes(text: string): Attribute[] {
  const out: Attribute[] = [];
  for (const line of text.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (trimmed === "") continue;
    const at = trimmed.search(/[=:：]/);
    const key = at < 0 ? trimmed : trimmed.slice(0, at);
    const value = at < 0 ? "" : trimmed.slice(at + 1);
    const item = { key: key.trim(), value: value.trim() };
    if (item.key === "" && item.value === "") continue;
    out.push(item);
  }
  return out;
}

/** 一串键值 → 那一栏的文本（`键 = 值`，一行一项）。 */
export function formatAttributes(attributes: Attribute[]): string {
  return attributes.map((attr) => t("entity.attribute_line", { key: attr.key, value: attr.value })).join("\n");
}

/** 一个筛选项：`all` 是"全部"，其余是某一类。 */
export interface EntityFilterOption {
  kind: EntityKind | "all";
  count: number;
}

/** 面板上的筛选项：**全部**在最前，跟着两种类型（一条都没有的也留着，好知道是 0）。 */
export function filterOptions(board: EntityBoard | null): EntityFilterOption[] {
  if (!board) return [{ kind: "all", count: 0 }];
  return [
    { kind: "all", count: board.cards.length },
    { kind: "person", count: board.persons },
    { kind: "setting", count: board.settings },
  ];
}

/** 按筛选项挑出要摆的那几张（`all` 原样返回）。 */
export function visibleCards(cards: EntityCard[], filter: EntityKind | "all"): EntityCard[] {
  if (filter === "all") return cards;
  return cards.filter((card) => card.kind === filter);
}

/**
 * 一张卡在列表里那句话：名字 +（有的话）别称。
 *
 * 别称只摆前两个——列表是**认脸**的地方，不是看全设定；点开编辑才看全部。
 */
export function cardHeadline(card: EntityCard): string {
  if (card.aliases.length === 0) return card.name;
  return t("common.name_with_aliases", {
    name: card.name,
    aliases: card.aliases.slice(0, 2).join(t("common.list_separator")),
  });
}

/** 一张卡上有几条设定（列表上那个小数字）。 */
export function attributeCount(card: EntityCard): number {
  return card.attributes.length;
}

/** 表单里那一份（**别名与设定是文本框里的原文**，存的时候才解析）。 */
export interface EntityDraft {
  /** `null` = 新建；有值 = 在改这一张 */
  id: number | null;
  kind: EntityKind;
  name: string;
  aliasesText: string;
  attributesText: string;
  note: string;
}

/** 新建一张的空白表单（默认人物——最常记的就是人）。 */
export function blankDraft(kind: EntityKind = "person"): EntityDraft {
  return { id: null, kind, name: "", aliasesText: "", attributesText: "", note: "" };
}

/** 把一张卡摊成表单（改它时用）。 */
export function draftOf(card: EntityCard): EntityDraft {
  return {
    id: card.id,
    kind: card.kind,
    name: card.name,
    aliasesText: formatAliases(card.aliases),
    attributesText: formatAttributes(card.attributes),
    note: card.note,
  };
}

/** 表单 → 交给核心的那一份（别名与设定在这一步解析）。 */
export function formOf(draft: EntityDraft): EntityCardForm {
  return {
    kind: draft.kind,
    name: draft.name.trim(),
    aliases: parseAliases(draft.aliasesText),
    attributes: parseAttributes(draft.attributesText),
    note: draft.note.trim(),
  };
}

/** 这张表单能不能交（**名字是身份**：空着不让交）。 */
export function draftReady(draft: EntityDraft): boolean {
  return draft.name.trim() !== "";
}
