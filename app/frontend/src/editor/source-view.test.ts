// 状态栏「这一章的字从哪来」：只露要留意的几档、顺序固定、口径跟着状态栏。
import { test } from "node:test";
import assert from "node:assert/strict";

import { sourceHighlights, sourceLabelKey, sourceRows } from "./source-view.ts";
import type { SourceBreakdown } from "../api/source.ts";

const BREAKDOWN: SourceBreakdown = {
  shares: [
    { source: "unknown", char_count: 10, chars_no_punct: 8, word_count: 6 },
    { source: "pasted", char_count: 300, chars_no_punct: 280, word_count: 260 },
    { source: "typed", char_count: 900, chars_no_punct: 800, word_count: 700 },
  ],
};

test("状态栏只露**非键入**的档（粘贴 / 未记录），全键入时整行不出现", () => {
  const lines = sourceHighlights(BREAKDOWN, "chars");
  assert.deepEqual(lines, [
    { source: "pasted", count: 300 },
    { source: "unknown", count: 10 },
  ]);
  assert.deepEqual(
    sourceHighlights({ shares: [{ source: "typed", char_count: 5, chars_no_punct: 5, word_count: 5 }] }, "chars"),
    [],
    "整章都是打出来的：不该有一行字提醒作者在打字",
  );
});

test("顺序照核心的固定次序，不管核心里给的先后", () => {
  assert.deepEqual(
    sourceRows(BREAKDOWN, "chars").map((line) => line.source),
    ["typed", "pasted", "unknown"],
  );
});

test("口径跟着状态栏：按词就报词数", () => {
  assert.deepEqual(sourceRows(BREAKDOWN, "words"), [
    { source: "typed", count: 700 },
    { source: "pasted", count: 260 },
    { source: "unknown", count: 6 },
  ]);
  assert.deepEqual(sourceHighlights(BREAKDOWN, "words"), [
    { source: "pasted", count: 260 },
    { source: "unknown", count: 6 },
  ]);
});

test("零字的档不露；认不出的码不往界面上抛英文", () => {
  assert.deepEqual(
    sourceRows({ shares: [{ source: "voice", char_count: 0, chars_no_punct: 0, word_count: 0 }] }, "chars"),
    [],
  );
  assert.equal(sourceLabelKey("pasted"), "editor.source.pasted");
  assert.equal(sourceLabelKey("hologram"), "editor.source.other");
});
