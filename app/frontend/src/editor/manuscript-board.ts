// 稿件台的**状态机**：看板开合、选中哪一篇、表单、以及那条"先说明后果"的判词。
//
// 分工（与书架同一条）：
// - 纯规则（列序 / 拖拽合法性 / 未完成散篇 / 日期）在 `manuscript.ts`，能脱离界面单测；
// - **真命令由会话层注入**（这里只认接口）——所以这一层也能脱开核心与 Tauri 单测；
// - 一条纪律与核心一致：**一稿多投是软拦**——判词不是 clear 时先把后果摆出来，
//   作者点过"仍然记下"才真的写；这里不替作者做决定。

import { computed, ref, type ComputedRef, type Ref } from "vue";

import type {
  ManuscriptBoard,
  ManuscriptDetail,
  ManuscriptRow,
  ManuscriptStatus,
  ManuscriptSubmitResult,
  Submission,
  SubmitVerdict,
} from "../api/manuscript";
import { overdueRows, statusKey } from "./manuscript";
import { t } from "../locales/index.ts";

/** 稿件台要用的动作（会话层注入真命令，测试注入替身）。 */
export interface ManuscriptTransport {
  board: () => Promise<ManuscriptBoard>;
  detail: (work_id: number) => Promise<ManuscriptDetail>;
  verdict: (work_id: number, venue: string) => Promise<SubmitVerdict>;
  submit: (
    work_id: number,
    venue: string,
    submitted_at: number,
    cooldown_until: number | null,
    notes: string,
  ) => Promise<ManuscriptSubmitResult>;
  move: (work_id: number, to: ManuscriptStatus) => Promise<ManuscriptDetail>;
  updateSubmission: (
    submission_id: number,
    venue: string,
    submitted_at: number,
    cooldown_until: number | null,
    notes: string,
  ) => Promise<ManuscriptDetail>;
  deleteSubmission: (submission_id: number) => Promise<ManuscriptDetail>;
  setTarget: (work_id: number, words: number | null) => Promise<number | null>;
}

/** 记一次投稿那张表单填的东西（时间都是毫秒）。 */
export interface SubmitDraft {
  venue: string;
  submittedAt: number;
  cooldownUntil: number | null;
  notes: string;
}

/** 正开着的那张表单：`submission` 为 null = 记一次新的，否则是改那一条。 */
export interface ManuscriptForm {
  work_id: number;
  submission: Submission | null;
}

/** 软拦：判词不是 `clear` 时先摆出来，作者点过「仍然记下」才真的写。 */
export interface WarnedSubmit {
  work_id: number;
  draft: SubmitDraft;
  verdict: SubmitVerdict;
}

export interface ManuscriptOptions {
  transport: ManuscriptTransport;
  /** 切到某一篇去写；返回是否真的切过去了（切不动就别收起稿件台） */
  openWork: (work_id: number) => Promise<boolean>;
  onError?: (message: string) => void;
}

export interface ManuscriptBoardState {
  visible: Ref<boolean>;
  busy: Ref<boolean>;
  /** 上一次动作的交代（"已记下投给某刊"之类） */
  note: Ref<string>;
  rows: Ref<ManuscriptRow[]>;
  /** 界面填表要用的默认冷冻期（核心给的，界面不自己抄一个 90 天） */
  defaultCooldownDays: Ref<number>;
  /**
   * 投稿已超期、可以改投的篇数。
   *
   * 它是**顶栏那个数字徽标**与稿件台里那条提示唯一的源——一处算、两处读，
   * 免得徽标说 2 篇、打开一看只有 1 篇（两份口径迟早分家）。
   */
  overdueCount: ComputedRef<number>;
  /** 正在看详情的那一篇（null = 没选） */
  detail: Ref<ManuscriptDetail | null>;
  form: Ref<ManuscriptForm | null>;
  warned: Ref<WarnedSubmit | null>;
  toggle: () => void;
  close: () => void;
  refresh: () => Promise<void>;
  select: (work_id: number) => Promise<void>;
  openSubmit: (work_id: number) => void;
  openEdit: (work_id: number, submission: Submission) => void;
  closeForm: () => void;
  /** 表单点「记下」：先问一句（判词），clear 就直接记，不 clear 就把后果摆出来 */
  save: (work_id: number, draft: SubmitDraft) => Promise<void>;
  /** 改一条已有记录（不判词：改的是既有事实，不是又投了一次） */
  saveEdit: (work_id: number, submission_id: number, draft: SubmitDraft) => Promise<void>;
  /** 判词不是 clear、但作者确认了：真的写 */
  confirmWarned: () => Promise<void>;
  move: (work_id: number, to: ManuscriptStatus) => Promise<void>;
  removeSubmission: (submission_id: number) => Promise<void>;
  setTarget: (work_id: number, words: number | null) => Promise<void>;
  openWork: (work_id: number) => Promise<void>;
}

export function useManuscript(options: ManuscriptOptions): ManuscriptBoardState {
  const visible = ref(false);
  const busy = ref(false);
  const note = ref("");
  const rows = ref<ManuscriptRow[]>([]);
  const defaultCooldownDays = ref(90);
  const detail = ref<ManuscriptDetail | null>(null);
  const form = ref<ManuscriptForm | null>(null);
  const warned = ref<WarnedSubmit | null>(null);

  /** 超期未回的篇数：打开稿件台之前也能在顶栏看到（见 `refresh` 那次开机预热）。 */
  const overdueCount = computed(() => overdueRows(rows.value).length);

  const report = (error: unknown) => {
    options.onError?.(error instanceof Error ? error.message : String(error));
  };

  async function refresh(): Promise<void> {
    try {
      const board = await options.transport.board();
      rows.value = board.rows;
      defaultCooldownDays.value = board.default_cooldown_days;
    } catch (error) {
      report(error);
    }
  }

  /** 把一次"会改稿件台的动作"包起来：忙标记 + 刷新 + 报错，一处收口。 */
  async function act(op: () => Promise<void>): Promise<void> {
    if (busy.value) return;
    busy.value = true;
    try {
      await op();
      await refresh();
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  /** 写完一条记录之后：顺带把"正在看的那一篇"也对上（不然详情是旧的）。 */
  async function afterWrite(work_id: number): Promise<void> {
    detail.value = await options.transport.detail(work_id);
  }

  async function writeSubmission(work_id: number, draft: SubmitDraft): Promise<void> {
    await options.transport.submit(
      work_id,
      draft.venue,
      draft.submittedAt,
      draft.cooldownUntil,
      draft.notes,
    );
    form.value = null;
    warned.value = null;
    note.value = t("manuscript.note_submitted", { venue: draft.venue });
    await afterWrite(work_id);
  }

  return {
    visible,
    busy,
    note,
    rows,
    defaultCooldownDays,
    overdueCount,
    detail,
    form,
    warned,
    toggle: () => {
      visible.value = !visible.value;
      if (visible.value) void refresh();
    },
    close: () => {
      visible.value = false;
      form.value = null;
      warned.value = null;
    },
    refresh,
    select: async (work_id) => {
      try {
        detail.value = await options.transport.detail(work_id);
      } catch (error) {
        report(error);
      }
    },
    openSubmit: (work_id) => {
      form.value = { work_id, submission: null };
      warned.value = null;
    },
    openEdit: (work_id, submission) => {
      form.value = { work_id, submission };
      warned.value = null;
    },
    closeForm: () => {
      form.value = null;
      warned.value = null;
    },
    save: (work_id, draft) =>
      act(async () => {
        const verdict = await options.transport.verdict(work_id, draft.venue);
        if (verdict.code !== "clear") {
          // 软拦：先把后果摆出来，作者点「仍然记下」再写
          warned.value = { work_id, draft, verdict };
          return;
        }
        await writeSubmission(work_id, draft);
      }),
    saveEdit: (work_id, submission_id, draft) =>
      act(async () => {
        await options.transport.updateSubmission(
          submission_id,
          draft.venue,
          draft.submittedAt,
          draft.cooldownUntil,
          draft.notes,
        );
        form.value = null;
        note.value = t("manuscript.note_record_saved");
        await afterWrite(work_id);
      }),
    confirmWarned: () =>
      act(async () => {
        const pending = warned.value;
        if (!pending) return;
        await writeSubmission(pending.work_id, pending.draft);
      }),
    move: (work_id, to) =>
      act(async () => {
        const before = rows.value.find((row) => row.id === work_id)?.status;
        await options.transport.move(work_id, to);
        if (before) {
          note.value = t("manuscript.note_moved", {
            from: t(statusKey(before)),
            to: t(statusKey(to)),
          });
        }
        if (detail.value?.work_id === work_id) await afterWrite(work_id);
      }),
    removeSubmission: (submission_id) =>
      act(async () => {
        const work_id = detail.value?.work_id;
        await options.transport.deleteSubmission(submission_id);
        note.value = t("manuscript.note_record_deleted");
        if (work_id !== undefined) await afterWrite(work_id);
      }),
    setTarget: (work_id, words) =>
      act(async () => {
        await options.transport.setTarget(work_id, words);
        note.value =
          words === null || words <= 0
            ? t("manuscript.note_target_cleared")
            : t("manuscript.note_target", { words });
        if (detail.value?.work_id === work_id) await afterWrite(work_id);
      }),
    openWork: async (work_id) => {
      try {
        if (await options.openWork(work_id)) visible.value = false;
      } catch (error) {
        report(error);
      }
    },
  };
}
