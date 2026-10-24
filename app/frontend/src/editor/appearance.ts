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

import { ref, watch, type Ref } from "vue";

import type { Appearance, AppearancePatch, NamingRewrite } from "../api/core";
import type { BackgroundOutcome } from "../api/background";
import { t } from "../locales/index.ts";
import type { TypographyField } from "./typography";

/** 右侧第二栏露哪一块（与核心 `ASIDE_TABS` 的稳定码一致）。 */
export type AsideTab = "flow" | "creator" | "sop";

/** 识别第二栏分区码（认不出的给 null）——**闭集只此一处**，别处不许再写一份。 */
export function parseAsideTab(value: string): AsideTab | null {
  return value === "flow" || value === "creator" || value === "sop" ? value : null;
}

/** 偏好要用的几个动作（会话层注入真命令，测试注入替身）。 */
export interface AppearanceTransport {
  read: (work_id: number | null) => Promise<Appearance>;
  write: (work_id: number | null, patch: AppearancePatch) => Promise<Appearance>;
  reset: (work_id: number | null) => Promise<Appearance>;
  /** 编号写法：先看会改哪几章、改成什么（只算不改） */
  previewNaming: (work_id: number) => Promise<NamingRewrite[]>;
  /** 编号写法：照预览那份清单执行（作者点过确认才走到这里） */
  applyNaming: (work_id: number, rewrites: NamingRewrite[]) => Promise<number>;
  /** 正文背景图：读现状 / 请作者挑一张 / 清掉（文件那一半全在壳里） */
  backgroundImage: () => Promise<BackgroundOutcome>;
  backgroundPick: (title: string, filter_label: string) => Promise<BackgroundOutcome>;
  backgroundClear: () => Promise<BackgroundOutcome>;
}

export interface AppearanceOptions {
  transport: AppearanceTransport;
  /** 当前作品（null = 还没打开书）——"只设这本书"要用它 */
  workId: Ref<number | null>;
  onError?: (message: string) => void;
}

/** 设命名规则时写哪一层：全局默认 / 只设当前这本书。 */
export type NamingTarget = "default" | "work";

export interface AppearanceState {
  /** 库里那份（**读失败或还没读是 null**）；设置面板据此显示 */
  values: Ref<Appearance | null>;
  /** **当前作品生效的那一份**（全局 + 每书覆盖合并后）；没打开书时是 null */
  workValues: Ref<Appearance | null>;
  /** 设置面板开着没有 */
  visible: Ref<boolean>;
  busy: Ref<boolean>;
  /** 读一次（启动时用；失败不打扰写作，按安全那一边走） */
  load: () => Promise<void>;
  /** 打开设置面板（打开前重读一次，免得显示旧值） */
  open: () => Promise<void>;
  close: () => void;
  /** 改一项：写完回读，界面显示的永远是库里那份 */
  setJumpToEnd: (value: boolean) => Promise<void>;
  /** 引号用哪一套（排版清理里选的，选一次就记住） */
  setQuoteStyle: (value: string) => Promise<void>;
  /**
   * 叩问问话的语气（`warm` / `neutral` / `direct`；传 `"auto"` = 清掉这一层，回默认"温柔"）。
   *
   * 只写**全局默认**（与排版三项同一条口径：每书覆盖的机制留着，第一版界面不暴露）。
   */
  setQuestionTone: (value: string) => Promise<void>;
  /**
   * 主动问一句的打扰度：每天最多几次（**0 = 不打扰**）+ 冷却多少分钟。
   *
   * 传 `null` = 把这一层清掉、回默认（3 次 / 60 分钟）。同样只写全局默认。
   */
  setQuestionPush: (perDay: number | null, cooldownMinutes: number | null) => Promise<void>;
  /**
   * 右侧第二栏露哪一块（`flow` = 叩问 / `creator` = 创作流）——**记住上次**。
   *
   * 只写全局那一层（与排版三项、叩问语气同一条口径：每书覆盖的机制留着，界面不暴露）。
   * 它只决定那一栏先露哪一块，不影响任何数据。
   */
  setAsideTab: (value: string) => Promise<void>;
  /**
   * 换一档界面配色（`light` / `dark` / `system`；传 `"auto"` = 清掉这一层、回默认浅色）。
   *
   * **只写全局那一层**（跟眼睛与房间光线有关，跟哪本书无关）——
   * 核心那边这项不参与每书覆盖，所以这里也不给"只设这本书"的口子。
   */
  setTheme: (value: string) => Promise<void>;
  /**
   * 护眼滤色的幅度（0~100；**`0` = 关**，一个像素都不染）。**只写全局那一层**
   * （跟眼睛与房间光线有关，跟哪本书无关）——核心那边这项不参与每书覆盖，
   * 所以这里也不给"只设这本书"的口子。
   *
   * 它只改屏幕观感：整窗铺一层暖黄的膜，正文、导出与落盘字节一个都不动。
   * 深浅两套配色合计出来的上限由界面算（见 `editor/eye-care.ts`）。
   */
  setEyeCare: (value: number) => Promise<void>;
  /**
   * 用眼提醒：开关 + 两个阈值（连着写满多少分钟 / 今天写到多少字）。
   *
   * **只写全局那一层**（与配色同一条口径：这两个数与哪本书无关）——所以这里也不给
   * "只设这本书"的口子。两个阈值 ≤0 或超上限会被核心当场拒，界面照单报出来
   * （它要拿去跟"写了多久 / 写了多少"比，一个 0 会让它每时每刻都响，那不是提醒）。
   */
  setBreakReminder: (enabled: boolean, minutes: number, words: number) => Promise<void>;
  /** 回到默认（核心的默认值） */
  resetToDefault: () => Promise<void>;
  /** 跟当前作品重读一次（打开章节、换书时用） */
  loadWork: () => Promise<void>;
  /** 设命名规则：`target` 决定写全局默认还是只写这本书；值传 `"auto"` = 清掉那一层 */
  setNaming: (value: string, target: NamingTarget) => Promise<void>;
  /**
   * 设"章的号跨不跨卷数"（continue / per_volume；传 "auto" = 清掉这一层）。
   *
   * 它改的只是**渲染时怎么数**，所以没有"整本一起换"那回事：写进去立刻全见效，
   * 标题与正文一个字不动（跟命名写法那一条不一样，这里不必先预览）。
   */
  setChapterNumbering: (value: string, target: NamingTarget) => Promise<void>;
  /**
   * 设正文排版的一项（字号 / 行距 / 字距）：**只写全局那一层**（第一版先只暴露全局，
   * 每书覆盖的机制已经在了，将来给面板加个"只设这本书"就能用）。
   *
   * 传 `0` = 清掉这一项（回默认档）。它只影响观感：写完编辑区立刻变，正文一个字不动。
   */
  setTypography: (field: TypographyField, value: number) => Promise<void>;
  /**
   * 设稿纸横线的线型（`solid` / `dashed`；传 `"auto"` = 清掉这一层，回默认"不显示"）。
   *
   * 与排版三项同一条口径：**只写全局那一层**（第一版先只暴露全局，每书覆盖的机制已经在了）。
   * 它只影响观感：写完编辑区立刻变，正文一个字不动。
   */
  setRuleStyle: (value: string) => Promise<void>;
  /**
   * 正文背景：底色 / 底纹 / 图片层的不透明度与铺法。
   *
   * 与排版三项同一条口径：**只写全局那一层**（第一版先只暴露全局，每书覆盖的机制已经在了）。
   * 纯观感——写完编辑区立刻变，正文一个字不动，也不进导出。
   *
   * 传 `"auto"`（底色 / 底纹 / 铺法）= 清掉这一层、回默认；不透明度传**负数** = 清掉这一层
   * （`0` 是正经取值：图留着但看不见）。
   */
  setBackgroundColor: (value: string) => Promise<void>;
  setBackgroundLayout: (value: string) => Promise<void>;
  setBackgroundFit: (value: string) => Promise<void>;
  setBackgroundOpacity: (value: number) => Promise<void>;
  /** 现在的背景图长什么样（文件名 / 能不能画 / 多少字节）——打开面板时问一次 */
  background: Ref<BackgroundOutcome | null>;
  /** 请作者挑一张图（壳复制进数据目录；取消时什么都不变） */
  pickBackground: () => Promise<void>;
  /** 清掉背景图（库里清空 + 数据目录里那几张删掉） */
  clearBackground: () => Promise<void>;
  /** 问一次"现在的背景图还在不在"（比如刚打开设置面板时） */
  refreshBackground: () => Promise<void>;
  /** 整本书换编号写法的清单（还没问回来是 null；空数组 = 不用换） */
  namingPlan: Ref<NamingRewrite[] | null>;
  /** 问一次"这本书哪些章会换" */
  previewNaming: () => Promise<void>;
  /** 执行（照那份清单），回来把目录树刷新交给调用方 */
  applyNaming: () => Promise<number | null>;
}

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

  const state: AppearanceState = {
    values,
    workValues,
    visible,
    busy,
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
      // 写全局那一层（稀疏合并：只传这一项，别的项不受影响）
      await act(() => options.transport.write(null, { [field]: value }));
      // 全局换了 → "这本书生效的那一份"也跟着换：顺手重读，编辑区与面板立刻一致
      await state.loadWork();
    },
    setRuleStyle: async (value) => {
      // 传 "auto" = 清掉这一层（核心那边就是"回默认：不显示"）；别的取值核心会校验。
      // 与 setTypography 同一条路：写全局那一层，再重读"这本书生效的那一份"。
      await act(() => options.transport.write(null, { editor_rule_style: value }));
      await state.loadWork();
    },
    setBackgroundColor: async (value) => {
      // 同上：`"auto"` = 清掉这一层（回默认纸白）；别的取值核心会校验
      await act(() => options.transport.write(null, { editor_bg_color: value }));
      await state.loadWork();
    },
    setBackgroundLayout: async (value) => {
      await act(() => options.transport.write(null, { editor_bg_layout: value }));
      await state.loadWork();
    },
    setBackgroundFit: async (value) => {
      await act(() => options.transport.write(null, { editor_bg_fit: value }));
      await state.loadWork();
    },
    setBackgroundOpacity: async (value) => {
      // 传负数 = 清掉这一层（回默认 35）；`0` 是正经取值（图留着但看不见），
      // 所以这里不拿 0 当"清掉"——那会把"先藏起来"变成一个改不掉的空档
      await act(() => options.transport.write(null, { editor_bg_opacity: value }));
      await state.loadWork();
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
    resetToDefault: () => act(() => options.transport.reset(null)),
  };

  // 换书：这一本有没有单独覆盖外观里的某项（比如正文排版），得重读一次才知道——
  // 编辑区绑的就是这份"这本书生效的"，不重读就会沿用上一本的观感。
  watch(options.workId, () => {
    void state.loadWork();
  });

  return state;
}
