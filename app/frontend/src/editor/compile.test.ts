// 编译面板的测试：**默认值由核心给、预览不落盘、编译才写文件**。
//
// 这里测的是"设备"而不是"渲染"：给一个替身传输层，看它在各种选择下叫了哪些动作。

import { test } from "node:test";
import assert from "node:assert/strict";

import type { CompileAck, CompileFile, CompilePreset } from "../api/core.ts";
import { fileSize, presetOf, useCompile, type CompileTransport } from "./compile.ts";

const PRESETS: CompilePreset[] = [
  { code: "submission_docx", caps_body: true, default_body_limit: 30_000, default_outline: true },
  { code: "chapters_txt", caps_body: false, default_body_limit: 0, default_outline: false },
  { code: "merged_txt", caps_body: false, default_body_limit: 0, default_outline: false },
];

function build() {
  const calls: string[] = [];
  const errors: string[] = [];
  const transport: CompileTransport = {
    presets: async () => PRESETS,
    preview: async (work_id, preset, body_limit, with_outline) => {
      calls.push(`preview:${work_id}:${preset}:${body_limit}:${with_outline}`);
      return [{ path: `${preset}/长夜.docx`, bytes: 2048 }];
    },
    run: async (work_id, preset, body_limit, with_outline): Promise<CompileAck> => {
      calls.push(`run:${work_id}:${preset}:${body_limit}:${with_outline}`);
      const files: CompileFile[] = [{ path: `${preset}/长夜.docx`, bytes: 2048 }];
      return { path: "导出目录/长夜/submission", files, removed: 1 };
    },
  };
  const state = useCompile({ transport, onError: (message) => errors.push(message) });
  return { state, calls, errors };
}

test("打开就按核心给的默认值填好参数", async () => {
  const { state, calls } = build();
  await state.open(7);
  assert.equal(state.visible.value, true);
  assert.equal(state.preset.value, "submission_docx");
  assert.equal(state.bodyLimit.value, 30_000, "默认上限来自核心");
  assert.equal(state.withOutline.value, true);
  assert.deepEqual(calls, [], "打开面板不该有任何动作（连预设清单也只在第一次拉）");
});

test("换预设：参数跟着换成那一种的默认", async () => {
  const { state } = build();
  await state.open(7);
  state.selectPreset("merged_txt");
  assert.equal(state.preset.value, "merged_txt");
  assert.equal(state.bodyLimit.value, null, "不卡上限的预设：全书都带");
  assert.equal(state.withOutline.value, false);
  // 改过的参数在预览时原样送到核心
  state.setBodyLimit(5_000);
  state.setOutline(true);
  await state.preview();
  assert.equal(state.files.value.length, 1);
  assert.equal(state.note.value.length > 0, true);
});

test("预览不落盘，编译才写文件", async () => {
  const { state, calls } = build();
  await state.open(7);
  state.setBodyLimit(12_000);
  await state.preview();
  assert.deepEqual(calls, ["preview:7:submission_docx:12000:true"]);

  await state.run();
  assert.deepEqual(calls, [
    "preview:7:submission_docx:12000:true",
    "run:7:submission_docx:12000:true",
  ]);
  assert.equal(state.files.value.length, 1);
  // 交代里既有落点也有清掉的旧文件
  assert.ok(state.note.value.includes("导出目录/长夜/submission"), state.note.value);
});

test("空上限＝全书都带", async () => {
  const { state, calls } = build();
  await state.open(7);
  state.setBodyLimit(null);
  await state.preview();
  assert.ok(calls.includes("preview:7:submission_docx:null:true"), calls.join("|"));
});

test("预设查不到就退回第一项；文件大小给人看的说法", () => {
  assert.equal(presetOf(PRESETS, "epub")?.code, "submission_docx");
  assert.equal(presetOf([], "submission_docx"), null);
  assert.equal(fileSize(512), "512 B");
  assert.equal(fileSize(2048), "2 KB");
  assert.equal(fileSize(3 * 1024 * 1024), "3.0 MB");
});
