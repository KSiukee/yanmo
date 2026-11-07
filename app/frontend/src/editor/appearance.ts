// 外观 / 写作行为偏好：**全局一份、跟人不跟书**（书可以单独覆盖，结构留位）。
//
// 三条分寸：
// - **只存改过的项**：库里是"空档"，读的时候才落默认值（默认值只写在核心那一处），
//   所以这里读到的永远是"已经落好值"的一份；
// - **没读回来之前不拿默认值当真相**：`values` 先是 null，读失败就一直 null——
//   界面此时按"不抢焦点"处理（安全那一边），而不是猜一个默认值往下走；
// - 这些偏好**只影响显示与手感**，不进导出、不改动正文一个字。
//
// 只认接口不认具体命令（真命令在会话层注入）：可以脱离界面与核心单测。

import { computed, ref, watch } from "vue";

import {
  CLEAR_VALUE,
  clearVisualPatch,
  hasVisualOverrides,
  visualPatchFrom,
} from "./appearance-work.ts";
export { WORK_VISUAL_FIELDS } from "./appearance-work.ts";
export type { WorkVisualField } from "./appearance-work.ts";

import type { Appearance, AppearancePatch, NamingRewrite } from "../api/core";
import type { BackgroundOutcome } from "../api/background";
import { t } from "../locales/index.ts";

// 偏好形状与注入点：单独成件（`appearance-contract.ts`），这里照旧转出。
export type {
  AsideTab,
  AppearanceOptions,
  AppearanceState,
  AppearanceTransport,
  NamingTarget,
} from "./appearance-contract.ts";
export { parseAsideTab } from "./appearance-contract.ts";

import type { AppearanceOptions, AppearanceState } from "./appearance-contract.ts";

export function useAppearance(options: AppearanceOptions): AppearanceState {
  const values = ref<Appearance | null>(null);
  const workValues = ref<Appearance | null>(null);
  const visible = ref(false);
  const namingPlan = ref<NamingRewrite[] | null>(null);
  /** 背景图现在长什么样（文件名 / 能不能画 / 多少字节）；**不是偏好**，是一次动作的回执 */
  const backgroundOutcome = ref<BackgroundOutcome | null>(null);
  const busy = ref(false);
  const report = (error: unknown) => {
    options.onError?.(error instanceof Error ? error.message : String(error));
  };

  /** 一次动作统一收口：忙标记 + 写回读 + 报错。 */
  async function act(op: () => Promise<Appearance>): Promise<void> {
    if (busy.value) return;
    busy.value = true;
    try {
      values.value = await op();
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  /** 写到"这一层"：全局写回 `values`，每书写回 `workValues`——两份不能互相覆盖。 */
  async function actOn(work: boolean, op: () => Promise<Appearance>): Promise<void> {
    if (busy.value) return;
    busy.value = true;
    try {
      const written = await op();
      if (work) workValues.value = written;
      else values.value = written;
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  const workMode = computed(() => hasVisualOverrides(workValues.value?.overridden));

  /** 观感项写哪一层：开关开着就写这一本，否则写全局。 */
  async function writeVisual(patch: AppearancePatch): Promise<void> {
    const work = workMode.value ? options.workId.value : null;
    if (workMode.value && work === null) return;
    await actOn(work !== null, () => options.transport.write(work, patch));
    // 全局换了、或这一层刚清过某项：都重读一次"这本书生效的那份"，面板才不显示旧值
    await state.loadWork();
  }

  const state: AppearanceState = {
    values,
    workValues,
    visible,
    busy,
    workMode,
    background: backgroundOutcome,
    load: async () => {
      try {
        values.value = await options.transport.read(null);
      } catch {
        values.value = null; // 读不到就一直 null：界面按"不抢焦点"处理
      }
    },
    open: async () => {
      if (values.value === null) await act(() => options.transport.read(null));
      // 打开设置时把"这本书生效的那一份"也重读一次（每书覆盖的项要显示当前值），
      // 并算一次"已有章节要不要换写法"——面板上那行提示一打开就是准的
      await state.loadWork();
      await state.previewNaming();
      // 背景图问一次"还在不在"：库里有名字但文件被删掉时，面板要说清"图没了，回纯色"
      await state.refreshBackground();
      visible.value = true;
    },
    close: () => {
      visible.value = false;
      namingPlan.value = null;
    },
    namingPlan,
    previewNaming: async () => {
      const work_id = options.workId.value;
      if (work_id === null) {
        namingPlan.value = null;
        return;
      }
      try {
        namingPlan.value = await options.transport.previewNaming(work_id);
      } catch (error) {
        report(error);
        namingPlan.value = null;
      }
    },
    applyNaming: async () => {
      const work_id = options.workId.value;
      const plan = namingPlan.value;
      if (work_id === null || plan === null || plan.length === 0) return null;
      busy.value = true;
      try {
        const changed = await options.transport.applyNaming(work_id, plan);
        namingPlan.value = null;
        return changed;
      } catch (error) {
        report(error);
        return null;
      } finally {
        busy.value = false;
      }
    },
    setJumpToEnd: (value) =>
      act(() => options.transport.write(null, { jump_to_end_on_latest: value })),
    setQuoteStyle: (value) => act(() => options.transport.write(null, { quote_style: value })),
    setQuestionTone: (value) =>
      act(() =>
        // "auto" 在核心那边就是"清掉这一层"（回默认温柔）；别的取值核心会校验
        options.transport.write(null, { question_tone: value }),
      ),
    setQuestionPush: (perDay, cooldownMinutes) =>
      act(() =>
        options.transport.write(null, {
          // null（＝没给）与负数在核心那边都表示"清掉这一层"；这里统一用 -1 表示清
          question_push_per_day: perDay === null ? -1 : perDay,
          question_push_cooldown_minutes: cooldownMinutes === null ? -1 : cooldownMinutes,
        }),
      ),
    setAsideTab: (value) =>
      act(() =>
        // "auto" 在核心那边就是"清掉这一层"（回默认叩问）；别的取值核心会校验
        options.transport.write(null, { aside_tab: value }),
      ),
    setTheme: (value) =>
      act(() =>
        // "auto" 在核心那边就是"清掉这一层"（回默认浅色）；别的取值核心会校验。
        // 只写全局（null）：配色不进每书覆盖。
        options.transport.write(null, { theme: value }),
      ),
    setEyeCare: (value) =>
      // 只写全局（null）：护眼滤色不进每书覆盖。`0` 是正经取值（＝关），所以不拿它当"清掉"——
      // 核心那边负数才是清掉这一层（与图片不透明度同一个口径）。
      act(() => options.transport.write(null, { eye_care: value })),
    setBreakReminder: (enabled, minutes, words) =>
      // 三项一次带上（与叩问那两个数同一条做法）：核心那边是稀疏合并，没传的不动。
      // 只写全局（null）：用眼提醒跟哪本书无关。
      act(() =>
        options.transport.write(null, {
          break_reminder: enabled,
          break_minutes: minutes,
          break_words: words,
        }),
      ),
    loadWork: async () => {
      const work_id = options.workId.value;
      if (work_id === null) {
        workValues.value = null;
        return;
      }
      try {
        workValues.value = await options.transport.read(work_id);
      } catch {
        workValues.value = null; // 读不到就不显示"这本书生效的是哪一档"，不猜
      }
    },
    setNaming: async (value, target) => {
      const work_id = target === "work" ? options.workId.value : null;
      if (target === "work" && work_id === null) return;
      await act(async () => {
        const written = await options.transport.write(work_id, { naming: value });
        // 这一本生效的那一份也跟着刷（设置面板同时显示"现在生效"）
        workValues.value = work_id === null ? workValues.value : written;
        // 规则一变，"已有章节要换哪些"也跟着变——顺手重算，面板上那行提示才是最新的
        const target = options.workId.value;
        namingPlan.value =
          target === null ? null : await options.transport.previewNaming(target);
        return written;
      });
    },
    setChapterNumbering: async (value, target) => {
      const work_id = target === "work" ? options.workId.value : null;
      if (target === "work" && work_id === null) return;
      await act(async () => {
        const written = await options.transport.write(work_id, { chapter_numbering: value });
        // 这一本生效的那一份也跟着刷（设置面板同时显示"现在生效"）
        workValues.value = work_id === null ? workValues.value : written;
        return written;
      });
    },
    setTypography: async (field, value) => {
      // 稀疏合并：只传这一项；写哪一层由"这本书单独一套"决定
      await writeVisual({ [field]: value } as AppearancePatch);
    },
    setEditorFont: async (value) => {
      // 传 `预设码` / `family:<名字>` 都行；`"auto"` = 清掉这一层（回默认宋体那档）。
      await writeVisual({ editor_font: value });
    },
    setEditorFontScope: async (value) => {
      await writeVisual({ editor_font_scope: value });
    },
    setRuleStyle: async (value) => {
      // 传 "auto" = 清掉这一层（核心那边就是"回默认：不显示"）；别的取值核心会校验。
      await writeVisual({ editor_rule_style: value });
    },
    setBackgroundColor: async (value) => {
      // 同上：`"auto"` = 清掉这一层（回默认纸白）；别的取值核心会校验
      await writeVisual({ editor_bg_color: value });
    },
    setBackgroundLayout: async (value) => {
      await writeVisual({ editor_bg_layout: value });
    },
    setBackgroundFit: async (value) => {
      await writeVisual({ editor_bg_fit: value });
    },
    setBackgroundOpacity: async (value) => {
      // 传负数 = 清掉这一层（回默认 35）；`0` 是正经取值（图留着但看不见），
      // 所以这里不拿 0 当"清掉"——那会把"先藏起来"变成一个改不掉的空档
      await writeVisual({ editor_bg_opacity: value });
    },
    refreshBackground: async () => {
      try {
        backgroundOutcome.value = await options.transport.backgroundImage();
      } catch (error) {
        // 读不到就当作"没有图"：这不是挡住设置面板的理由（面板别的项照样能用）
        backgroundOutcome.value = null;
        report(error);
      }
    },
    pickBackground: async () => {
      if (busy.value) return;
      busy.value = true;
      try {
        // 窗口标题与筛选项由界面给（**壳不产文案**）；取消时 `cancelled` 为 true
        const outcome = await options.transport.backgroundPick(
          t("settings.background_pick_title"),
          t("settings.background_pick_filter"),
        );
        backgroundOutcome.value = outcome;
        // 换图会顺手把这一项落库，所以"这本书生效的那一份"要重读一次
        await state.loadWork();
      } catch (error) {
        report(error);
      } finally {
        busy.value = false;
      }
    },
    clearBackground: async () => {
      if (busy.value) return;
      busy.value = true;
      try {
        backgroundOutcome.value = await options.transport.backgroundClear();
        await state.loadWork();
      } catch (error) {
        report(error);
      } finally {
        busy.value = false;
      }
    },
    setWorkMode: async (on) => {
      const work_id = options.workId.value;
      if (work_id === null) return;
      if (on) {
        const source = workValues.value ?? values.value;
        if (!source) return;
        const patch = visualPatchFrom(source);
        if (Object.keys(patch).length === 0) return;
        await actOn(true, () => options.transport.write(work_id, patch as AppearancePatch));
      } else {
        await actOn(true, () =>
          options.transport.write(work_id, clearVisualPatch() as AppearancePatch),
        );
      }
      await state.loadWork();
    },
    clearWorkField: async (field) => {
      const work_id = options.workId.value;
      if (work_id === null) return;
      await actOn(true, () =>
        options.transport.write(work_id, { [field]: CLEAR_VALUE[field] } as AppearancePatch),
      );
      await state.loadWork();
    },
    resetToDefault: () => act(() => options.transport.reset(null)),
  };

  // 换书：这一本有没有单独覆盖外观里的某项（比如正文排版），得重读一次才知道——
  // 编辑区绑的就是这份"这本书生效的"，不重读就会沿用上一本的观感。
  watch(options.workId, () => {
    void state.loadWork();
  });

  return state;
}
