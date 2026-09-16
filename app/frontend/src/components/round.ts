// 「先问后排版」那一轮攒着的答案：**纯列表操作**，能单测。
//
// 为什么单独一个文件：托盘里那点事（加一条 / 挪一位 / 删掉 / 改字 / 拼成一段）是纯数据变换，
// 与"什么时候去问核心"不是一回事。纯函数留在这一层，面板那一层只管调。
import type { RoundItem } from "../api/question.ts";

/**
 * 把刚答下的一条放进这一轮。
 *
 * 同一张卡只留一条（后加的那条替换旧的）：一张卡是终态、答案只有一份，
 * 托盘里出现两条同一张卡的会把"落了没有"搅浑。
 */
export function appendRound(items: RoundItem[], item: RoundItem): RoundItem[] {
  return [...items.filter((existing) => existing.card_id !== item.card_id), item];
}

/** 删掉一位（从这一轮移出；答案仍在答案池里，只是这一轮不落它）。 */
export function removeAt(items: RoundItem[], index: number): RoundItem[] {
  return items.filter((_, at) => at !== index);
}

/** 挪一位：`delta` 是 -1（往前）/ +1（往后）；挪不动就原样返回（不报错、不吞掉）。 */
export function moveAt(items: RoundItem[], index: number, delta: number): RoundItem[] {
  const to = index + delta;
  if (index < 0 || index >= items.length || to < 0 || to >= items.length) return items;
  const next = [...items];
  const [moved] = next.splice(index, 1);
  next.splice(to, 0, moved);
  return next;
}

/** 改一位的字（作者在托盘里顺手润一句）。 */
export function amendAt(items: RoundItem[], index: number, body: string): RoundItem[] {
  return items.map((item, at) => (at === index ? { ...item, body } : item));
}

/**
 * 把这一轮里**落正文**的那几条拼成一段文字：一次插进正文，段与段之间空一行。
 *
 * 只算正文：投章纲（这一章的一句话）与投场景卡（新建一张卡）的那几条**不进正文**——
 * 它们各归各的去处，混进来就成了"章纲也抄进正文一份"。
 * 空白的条目丢掉（作者删字删空了等于不要这一段）——不丢的话正文里会多出空段。
 */
export function roundText(items: RoundItem[]): string {
  return items
    .filter((item) => item.target === "" || item.target === "body")
    .map((item) => item.body.trim())
    .filter((body) => body !== "")
    .join("\n\n");
}

/** 这一轮里有没有要落进正文的（没有就别去动编辑器）。 */
export function roundHasBody(items: RoundItem[]): boolean {
  return items.some((item) => (item.target === "" || item.target === "body") && item.body.trim() !== "");
}

/** 这一轮有没有东西可落（全是空白等于没有）。 */
export function roundHasText(items: RoundItem[]): boolean {
  return items.some((item) => item.body.trim() !== "");
}
