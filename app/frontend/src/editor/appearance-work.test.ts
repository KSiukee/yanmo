// 「这本书单独一套」的行为验收：开关写哪一层、逐项回全局、关掉清干净。
//
// 只验编排（这是本轮新增的那段），真值落库与合并规则在核心（另有验收）。
import { test } from "node:test";
import assert from "node:assert/strict";
import { ref } from "vue";

import type { Appearance, AppearancePatch } from "../api/core";
import type { BackgroundOutcome } from "../api/background";
import { useAppearance } from "./appearance.ts";

function view(overridden: string[] = []): Appearance {
  return {
    jump_to_end_on_latest: true,
    word_count_caliber: null,
    quote_style: "curly",
    daily_goal: null,
    naming: "arabic",
    chapter_numbering: "continue",
    editor_font_size: null,
    editor_line_height: null,
    editor_letter_spacing: null,
    editor_font: "kai",
    editor_font_scope: "body",
    editor_rule_style: "none",
    question_tone: "warm",
    question_push_per_day: 3,
    question_push_cooldown_minutes: 60,
    aside_tab: "flow",
    theme: "light",
    break_reminder: true,
    break_minutes: 45,
    break_words: 1500,
    eye_care: 0,
    editor_bg_color: "paper",
    editor_bg_layout: "none",
    editor_bg_image: null,
    editor_bg_fit: "cover",
    editor_bg_opacity: 35,
    overridden,
  };
}

const NOTHING: BackgroundOutcome = {
  name: null,
  data_url: null,
  bytes: null,
  present: false,
  cancelled: false,
};

function harness() {
  const writes: Array<{ work_id: number | null; patch: AppearancePatch }> = [];
  const overrides = new Set<string>();
  const transport = {
    read: async (work_id: number | null) => view(work_id === null ? [] : [...overrides]),
    write: async (work_id: number | null, patch: AppearancePatch) => {
      writes.push({ work_id, patch });
      if (work_id !== null) {
        for (const [field, value] of Object.entries(patch)) {
          if (value === "auto" || value === 0 || value === -1) overrides.delete(field);
          else overrides.add(field);
        }
      }
      return view(work_id === null ? [] : [...overrides]);
    },
    reset: async () => {
      overrides.clear();
      return view([]);
    },
    previewNaming: async () => [],
    applyNaming: async () => 0,
    backgroundImage: async () => NOTHING,
    backgroundPick: async () => NOTHING,
    backgroundClear: async () => NOTHING,
  };
  const workId = ref<number | null>(1);
  const state = useAppearance({ transport, workId });
  return { state, writes, overrides, workId };
}

test("打开开关：把当前生效值写进这一本，不改全局", async () => {
  const { state, writes, overrides } = harness();
  await state.open();
  assert.equal(state.workMode.value, false, "一开始这本书没有覆盖");

  await state.setWorkMode(true);
  const write = writes.at(-1);
  assert.ok(write && write.work_id === 1, "必须写这本书那一层");
  assert.ok(overrides.has("editor_font"), "当前生效的字体被抄成这一本的覆盖");
  assert.equal(state.workMode.value, true);
});

test("开着开关时改观感项：只写这本书；逐项回全局只清这一项", async () => {
  const { state, writes, overrides } = harness();
  await state.open();
  await state.setWorkMode(true);

  await state.setTypography("editor_font_size", 22);
  assert.equal(writes.at(-1)?.work_id, 1);
  assert.equal(writes.at(-1)?.patch.editor_font_size, 22);
  assert.ok(overrides.has("editor_font_size"));

  await state.clearWorkField("editor_font_size");
  assert.equal(writes.at(-1)?.work_id, 1);
  assert.equal(writes.at(-1)?.patch.editor_font_size, 0, "0 = 清掉这一层");
  assert.ok(!overrides.has("editor_font_size"));
  assert.ok(overrides.has("editor_font"), "别的覆盖不能被一起清掉");
});

test("关掉开关：这一本的观感覆盖清干净，全局一个字不动", async () => {
  const { state, writes, overrides } = harness();
  await state.open();
  await state.setWorkMode(true);
  await state.setWorkMode(false);

  const write = writes.at(-1);
  assert.equal(write?.work_id, 1);
  assert.equal(write?.patch.editor_font, "auto");
  assert.equal(write?.patch.editor_bg_opacity, -1);
  assert.equal(overrides.size, 0, "观感这一层清空");
  assert.equal(state.workMode.value, false);
  assert.ok(
    writes.every((item) => item.work_id === null || item.work_id === 1),
    "没有写到不认识的书上",
  );
});
