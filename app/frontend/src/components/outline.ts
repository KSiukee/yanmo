// 大纲体检面板的**纯展示逻辑**：一条发现怎么念成人话、缺的格子怎么称呼、忽略怎么分堆。
//
// 单独成文件的原因与 `creator.ts` 同一条：这些都是纯函数，能单测；
// 组件只管把它们摆上去。句子一律从字典取（`outline.*`）——核心只给码与参数。

import { t } from "../locales/index.ts";
import type { OutlineIssue } from "../api/outline.ts";

/**
 * 一条发现念成人话。
 *
 * 模板键是 `outline.issue.<规则码>`（规则码里本来就带点，正好当命名空间）。
 * 字典里没有这条规则（核心那边先长了新规则）时：**照原样露规则码**——
 * 屏幕上出现一个 `scene.looks_wrong` 是一眼看得见的错，比显示成别的强。
 */
export function issueText(issue: OutlineIssue): string {
  const key = `outline.issue.${issue.rule}`;
  const text = t(key, { ...readableParams(issue), title: sceneTitle(issue) });
  return text === key ? issue.rule : text;
}

/**
 * 参数里"一串名字"怎么摆：核心给的是**中性分隔**（`,`），顿号 / 逗号由界面按字典拼
 * （核心零文案——标点也是文案，得跟语言走）。
 */
export function readableParams(issue: OutlineIssue): Record<string, string> {
  const params = { ...issue.params };
  if (typeof params.names === "string") {
    params.names = params.names
      .split(",")
      .map((name) => name.trim())
      .filter((name) => name !== "")
      .join(t("common.list_separator"));
  }
  // 故事时间那一栏：作者没写自由文本时用排序值顶一句（句子不该出现一个空的「」）
  for (const [key, orderKey] of [
    ["time", "order"],
    ["earlier_time", "earlier_order"],
  ] as const) {
    if (params[key] === "" && params[orderKey] !== undefined) {
      params[key] = t("outline.time_fallback", { order: params[orderKey] });
    }
  }
  return params;
}

/**
 * 场景卡那一格的名字：**没起名的场景卡**在句子里说"（还没起名的场景卡）"。
 *
 * 核心给的是树上那个名字（可能空着）——空着的时候界面上不能出现一个「」，
 * 那看着像坏了。
 */
export function sceneTitle(issue: OutlineIssue): string {
  const title = (issue.params.title ?? "").trim();
  return title === "" ? t("outline.issue.untitled_scene") : title;
}

/** 缺的那几格怎么称呼（稳定码 → 人话）；认不出的码照原样露出来。 */
export function fieldLabel(code: string): string {
  const key = `outline.field.${code}`;
  const text = t(key);
  return text === key ? code : text;
}

/** 一条发现里"缺了哪几格"那一行（`pov,goal` → `缺：视角、目标`）；不缺就是空串。 */
export function missingLine(issue: OutlineIssue): string {
  const codes = (issue.params.missing ?? "")
    .split(",")
    .map((code) => code.trim())
    .filter((code) => code !== "");
  if (codes.length === 0) return "";
  const fields = codes.map(fieldLabel).join(t("common.list_separator"));
  return t("outline.missing", { fields });
}

/** 一条发现是不是"已忽略"（面板据此把它归到另一堆）。 */
export function isDismissed(issue: OutlineIssue, dismissed: string[]): boolean {
  return dismissed.includes(issue.fingerprint);
}

/** 分成两堆：还要看的 / 已忽略的（两堆都保序）。 */
export function splitIssues(
  issues: OutlineIssue[],
  dismissed: string[],
): { open: OutlineIssue[]; known: OutlineIssue[] } {
  const open: OutlineIssue[] = [];
  const known: OutlineIssue[] = [];
  for (const issue of issues) {
    (isDismissed(issue, dismissed) ? known : open).push(issue);
  }
  return { open, known };
}

/** 锚点指向哪儿（`entity:3` / `scene:9` / `foreshadow:5`）；认不出的返回空（界面据此不摆按钮）。 */
export type AnchorKind = "entity" | "scene" | "foreshadow" | "";

export function anchorTarget(anchors: string[]): { kind: AnchorKind; id: number | null } {
  for (const anchor of anchors) {
    const [kind, raw] = anchor.split(":");
    const id = Number(raw);
    if ((kind === "entity" || kind === "scene" || kind === "foreshadow") && Number.isFinite(id)) {
      return { kind, id };
    }
  }
  return { kind: "", id: null };
}
