// 边写边存：**防抖落盘 + 写后读回校验**。
//
// 三条纪律（对应"不丢稿"这个承诺）：
// 1. **击键不跨边界**：`changed()` 只在壳内累积文本，停笔一段时间后才落盘一次；
// 2. **不重复写**：内容指纹没变时核心直接返回，多余调用不会产生任何写入；
// 3. **存下去才算数**：每几秒核对"库里的正文还是不是我手上这份"（**只比指纹，不搬正文**），
//    对不上或长时间存不下去 → 立即抢救（先留快照，再把库改回手上这份）并上报状态。
//
// 计时与传输都是注入的：测试里可以拿假时钟把时间拨快，不用真的等下去。

export type SaveStatus = "idle" | "pending" | "saving" | "saved" | "error" | "desync";

/** 界面直接绑这个对象。 */
export interface AutosaveState {
  status: SaveStatus;
  /** 给用户看的补充说明（失败原因 / 抢救说明） */
  detail: string;
  char_count: number;
  word_count: number;
  /** 本次会话是否发生过抢救；发生过就一直留着，提醒用户回头看一眼 */
  incident: string | null;
}

export interface SaveAck {
  char_count: number;
  word_count: number;
  fingerprint: string;
}

export interface AutosaveTransport {
  save(node_id: number, body: string): Promise<SaveAck>;
  fingerprint(node_id: number): Promise<string>;
  emergency(node_id: number, body: string, reason: string): Promise<SaveAck>;
}

/** 停笔多久落盘——任务口径 150–300ms，越界会被夹住（防抖调到 0 就等于逐键落盘）。 */
export const DEBOUNCE_RANGE = { min: 150, max: 300 } as const;
/** 多久做一次读回校验——任务口径 2–5 秒。 */
export const VERIFY_RANGE = { min: 2000, max: 5000 } as const;

const DEFAULT_DEBOUNCE_MS = 250;
const DEFAULT_VERIFY_MS = 3000;
const DEFAULT_STUCK_MS = 1500;

type TimerHandle = ReturnType<typeof setTimeout>;

export interface AutosaveDeps {
  node_id: number;
  transport: AutosaveTransport;
  onState?: (state: AutosaveState) => void;
  now?: () => number;
  schedule?: (fn: () => void, ms: number) => TimerHandle;
  cancel?: (handle: TimerHandle) => void;
  debounceMs?: number;
  verifyMs?: number;
  stuckMs?: number;
}

function clamp(value: number, min: number, max: number): number {
  if (!Number.isFinite(value)) return min;
  return Math.min(max, Math.max(min, value));
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export class Autosave {
  readonly node_id: number;
  readonly debounceMs: number;
  readonly verifyMs: number;
  readonly stuckMs: number;

  private readonly transport: AutosaveTransport;
  private readonly onState?: (state: AutosaveState) => void;
  private readonly now: () => number;
  private readonly schedule: (fn: () => void, ms: number) => TimerHandle;
  private readonly cancel: (handle: TimerHandle) => void;

  private status: SaveStatus = "idle";
  private detail = "";
  private charCount = 0;
  private wordCount = 0;
  private incident: string | null = null;

  /** 手上这一份正文与它的修订号（每次真实变更 +1） */
  private text = "";
  private rev = 0;
  /** 已经落盘的是哪一版 */
  private savedRev = 0;
  private savedFingerprint: string | null = null;

  private inFlight: Promise<void> | null = null;
  private saveTimer: TimerHandle | null = null;
  private verifyTimer: TimerHandle | null = null;
  private lastChangeAt = 0;
  private stopped = false;

  constructor(deps: AutosaveDeps) {
    this.node_id = deps.node_id;
    this.transport = deps.transport;
    this.onState = deps.onState;
    this.now = deps.now ?? (() => Date.now());
    this.schedule = deps.schedule ?? ((fn, ms) => setTimeout(fn, ms));
    this.cancel = deps.cancel ?? ((handle) => clearTimeout(handle));
    this.debounceMs = clamp(deps.debounceMs ?? DEFAULT_DEBOUNCE_MS, DEBOUNCE_RANGE.min, DEBOUNCE_RANGE.max);
    this.verifyMs = clamp(deps.verifyMs ?? DEFAULT_VERIFY_MS, VERIFY_RANGE.min, VERIFY_RANGE.max);
    this.stuckMs = deps.stuckMs ?? DEFAULT_STUCK_MS;
  }

  /** 载入一章之后建立比对基准（`ack` 来自打开编辑器时的返回）。 */
  attach(body: string, ack: SaveAck): void {
    this.text = body;
    this.rev = 0;
    this.savedRev = 0;
    this.savedFingerprint = ack.fingerprint;
    this.charCount = ack.char_count;
    this.wordCount = ack.word_count;
    this.setStatus(ack.fingerprint === "" ? "idle" : "saved");
    this.armVerify();
  }

  /** 编辑器每次变更（**在壳内**调用，不跨边界）。 */
  changed(text: string): void {
    if (this.stopped || text === this.text) return;
    this.text = text;
    this.rev += 1;
    this.lastChangeAt = this.now();
    this.setStatus("pending");
    this.armSave();
  }

  /** 立刻落盘（失焦 / 关窗前用，不等防抖）。 */
  async flush(): Promise<void> {
    this.cancelSaveTimer();
    if (this.rev === this.savedRev && !this.inFlight) return;
    await this.saveNow();
  }

  /** 立刻做一次读回校验（测试与"我现在就想确认一下"用）。 */
  async verifyNow(): Promise<void> {
    if (this.stopped || this.inFlight) return;

    // ① 有没落盘的改动：看看是不是卡住了
    if (this.rev > this.savedRev) {
      if (this.now() - this.lastChangeAt > this.stuckMs) {
        await this.rescue("stuck");
      }
      return;
    }

    // ② 手上这份已经落过盘：核对库里的还是不是它
    const baseline = this.savedFingerprint;
    if (baseline === null) return;
    let remote: string;
    try {
      remote = await this.transport.fingerprint(this.node_id);
    } catch (e) {
      this.setStatus("error", `读回校验失败：${messageOf(e)}`);
      return;
    }
    if (remote !== baseline) {
      await this.rescue("desync");
    }
  }

  /** 关掉：取消所有定时器，之后任何调用都不再动作。 */
  dispose(): void {
    this.stopped = true;
    this.cancelSaveTimer();
    if (this.verifyTimer !== null) {
      this.cancel(this.verifyTimer);
      this.verifyTimer = null;
    }
  }

  state(): AutosaveState {
    return {
      status: this.status,
      detail: this.detail,
      char_count: this.charCount,
      word_count: this.wordCount,
      incident: this.incident,
    };
  }

  // ── 内部 ────────────────────────────────────────────────────────────

  private armSave(): void {
    this.cancelSaveTimer();
    if (this.stopped) return;
    this.saveTimer = this.schedule(() => {
      void this.saveNow();
    }, this.debounceMs);
  }

  private cancelSaveTimer(): void {
    if (this.saveTimer !== null) {
      this.cancel(this.saveTimer);
      this.saveTimer = null;
    }
  }

  private saveNow(): Promise<void> {
    if (this.stopped) return Promise.resolve();
    // 串行化：一次只有一个写在飞——并发写同一章只会互相覆盖，没有任何好处
    if (this.inFlight) return this.inFlight;
    if (this.rev === this.savedRev) {
      this.setStatus("saved");
      return Promise.resolve();
    }

    const text = this.text;
    const rev = this.rev;
    this.setStatus("saving");

    const run = async (): Promise<void> => {
      try {
        const ack = await this.transport.save(this.node_id, text);
        this.savedFingerprint = ack.fingerprint;
        this.savedRev = rev;
        this.charCount = ack.char_count;
        this.wordCount = ack.word_count;
      } catch (e) {
        this.setStatus("error", `保存失败：${messageOf(e)}`);
        this.armSave(); // 还脏着：过一会儿再试
        return;
      } finally {
        this.inFlight = null;
      }
      if (rev === this.rev) {
        this.setStatus("saved");
      } else {
        // 写回期间作者又改了：继续排队，不能把最后这几次击键留在内存里
        this.setStatus("pending");
        this.armSave();
      }
    };

    this.inFlight = run();
    return this.inFlight;
  }

  /** 抢救：手上这份先留快照，再逼着库回到这一版。 */
  private async rescue(reason: "stuck" | "desync"): Promise<void> {
    const text = this.text;
    const rev = this.rev;
    const label = reason === "stuck" ? "落盘长时间没有完成" : "库里的正文与手上这份不一致";
    this.incident = label;
    this.setStatus("desync", `${label}——已留快照并回写`);
    try {
      const ack = await this.transport.emergency(this.node_id, text, reason);
      this.savedFingerprint = ack.fingerprint;
      this.savedRev = rev;
      this.charCount = ack.char_count;
      this.wordCount = ack.word_count;
      if (rev === this.rev) {
        this.setStatus("saved", `${label}——已恢复`);
      } else {
        this.setStatus("pending");
        this.armSave();
      }
    } catch (e) {
      this.setStatus("error", `抢救失败：${messageOf(e)}`);
    }
  }

  private armVerify(): void {
    if (this.stopped || this.verifyTimer !== null) return;
    this.verifyTimer = this.schedule(() => {
      this.verifyTimer = null;
      void this.verifyNow().finally(() => this.armVerify());
    }, this.verifyMs);
  }

  private setStatus(status: SaveStatus, detail = ""): void {
    this.status = status;
    this.detail = detail;
    this.onState?.(this.state());
  }
}
