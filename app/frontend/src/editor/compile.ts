// 编译：**先说清会生成哪些文件，再动手**。
//
// 三条分寸与别处一致：
// - 渲染与"生成什么"全在核心（纯函数），这里只转发与管状态；
// - 预设参数（默认上限、要不要大纲）**由核心给**，界面不抄一份默认值；
// - 「看看会生成什么」是预览（不落盘），「编译」才写文件——作者先看清再动手。
//
// 只认接口不认具体命令（真命令在会话层注入）：这套状态机可以脱离界面与核心单测。

import { ref, type Ref } from "vue";

import type { CompileAck, CompileFile, CompilePreset } from "../api/core";
import { t } from "../locales/index.ts";

/** 编译要用的几个动作（会话层注入真命令，测试注入替身）。 */
export interface CompileTransport {
  presets: () => Promise<CompilePreset[]>;
  preview: (
    work_id: number,
    preset: string,
    body_limit: number | null,
    with_outline: boolean,
  ) => Promise<CompileFile[]>;
  run: (
    work_id: number,
    preset: string,
    body_limit: number | null,
    with_outline: boolean,
  ) => Promise<CompileAck>;
}

export interface CompileOptionsIn {
  transport: CompileTransport;
  onError?: (message: string) => void;
}

export interface CompileState {
  visible: Ref<boolean>;
  /** 预设清单（核心给的：代码 + 默认参数） */
  presets: Ref<CompilePreset[]>;
  /** 选中的预设代码 */
  preset: Ref<string>;
  /** 正文上限（`null` = 全书都带；只有卡上限的预设才用得上） */
  bodyLimit: Ref<number | null>;
  withOutline: Ref<boolean>;
  /** 预览结果：会生成哪些文件（空数组＝还没预览过） */
  files: Ref<CompileFile[]>;
  note: Ref<string>;
  busy: Ref<boolean>;
  /** 打开：带上要编译的那本书，并把参数拉回这一种预设的默认值 */
  open: (work_id: number) => Promise<void>;
  close: () => void;
  selectPreset: (code: string) => void;
  setBodyLimit: (limit: number | null) => void;
  setOutline: (on: boolean) => void;
  preview: () => Promise<void>;
  run: () => Promise<void>;
}

/** 当前选中的那一种预设（核心给的清单里查；查不到就是第一项）。 */
export function presetOf(presets: CompilePreset[], code: string): CompilePreset | null {
  return presets.find((preset) => preset.code === code) ?? presets[0] ?? null;
}

/** 文件大小的说法（给作者看的，别摆一串字节数）。 */
export function fileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function useCompile(options: CompileOptionsIn): CompileState {
  const visible = ref(false);
  const presets = ref<CompilePreset[]>([]);
  const preset = ref("submission_docx");
  const bodyLimit = ref<number | null>(null);
  const withOutline = ref(true);
  const files = ref<CompileFile[]>([]);
  const note = ref("");
  const busy = ref(false);
  /** 要编译的那本书（打开面板时定下来） */
  let workId: number | null = null;

  const report = (error: unknown) =>
    options.onError?.(error instanceof Error ? error.message : String(error));

  /** 换成某一种预设：参数回到**核心给的默认值**（界面不自己编默认）。 */
  function applyPreset(code: string): void {
    const chosen = presetOf(presets.value, code);
    if (!chosen) return;
    preset.value = chosen.code;
    bodyLimit.value = chosen.caps_body ? chosen.default_body_limit : null;
    withOutline.value = chosen.default_outline;
    files.value = [];
    note.value = "";
  }

  /** 当前参数（三处调用共用；`body_limit` 只有卡上限的预设才传）。 */
  function args(): [number, string, number | null, boolean] {
    return [workId ?? 0, preset.value, bodyLimit.value, withOutline.value];
  }

  return {
    visible,
    presets,
    preset,
    bodyLimit,
    withOutline,
    files,
    note,
    busy,
    open: async (id) => {
      workId = id;
      if (presets.value.length === 0) {
        try {
          presets.value = await options.transport.presets();
        } catch (error) {
          report(error);
          return;
        }
      }
      applyPreset(preset.value);
      visible.value = true;
    },
    close: () => {
      visible.value = false;
    },
    selectPreset: (code) => applyPreset(code),
    setBodyLimit: (limit) => {
      bodyLimit.value = limit;
      files.value = [];
    },
    setOutline: (on) => {
      withOutline.value = on;
      files.value = [];
    },
    preview: async () => {
      if (busy.value || workId === null) return;
      busy.value = true;
      try {
        files.value = await options.transport.preview(...args());
        note.value =
          files.value.length > 0 ? t("compile.preview_result", { count: files.value.length }) : t("compile.empty");
      } catch (error) {
        report(error);
      } finally {
        busy.value = false;
      }
    },
    run: async () => {
      if (busy.value || workId === null) return;
      busy.value = true;
      try {
        const ack = await options.transport.run(...args());
        files.value = ack.files;
        note.value = t("compile.done", { count: ack.files.length, path: ack.path });
        if (ack.removed > 0) {
          note.value += ` ${t("compile.cleaned", { count: ack.removed })}`;
        }
      } catch (error) {
        report(error);
      } finally {
        busy.value = false;
      }
    },
  };
}
