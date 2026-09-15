// 边写边存：**防抖落盘 + 写后读回校验**。
//
// 三条纪律（对应"不丢稿"这个承诺）：
// 1. **击键不跨边界**：`changed()` 只在壳内累积文本，停笔一段时间后才落盘一次；
// 2. **不重复写**：内容指纹没变时核心直接返回，多余调用不会产生任何写入；
// 3. **存下去才算数**：每几秒核对"库里的正文还是不是我手上这份"（**只比指纹，不搬正文**），
//    对不上或长时间存不下去 → 立即抢救（先留快照，再把库改回手上这份）并上报状态。
//
// 计时与传输都是注入的：测试里可以拿假时钟把时间拨快，不用真的等下去。

import { t } from "../locales/index.ts";

export type SaveStatus = "idle" | "pending" | "saving" | "saved" | "error" | "desync";

/** 界面直接绑这个对象。 */
export interface AutosaveState {
  status: SaveStatus;
  /** 给用户看的补充说明（失败原因 / 抢救说明） */
  detail: string;
  char_count: number;
  chars_no_punct: number;
  word_count: number;
  /** 本次会话是否发生过抢救；发生过就一直留着，提醒用户回头看一眼 */
  incident: string | null;
}

export interface SaveAck {
  char_count: number;
  chars_no_punct: number;
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
/** `flush()` 最多排几笔：第一笔可能在飞、这期间作者又改了，得再排一笔；排满还脏就报失败，不空转。 */
const FLUSH_ROUNDS = 3;

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
  private charsNoPunct = 0;
  private wordCount = 0;
  private incident: string | null = null;

  /** 手上这一份正文与它的修订号（每次真实变更 +1） */
  private text = "";
  private rev = 0;
  /** 已经落盘的是哪一版 */
  private savedRev = 0;
  private savedFingerprint: string | null = null;

  private inFlight: Promise<void> | null = null;
  /** 在飞的那一笔是什么时候起的——用来判断它是不是挂住了（评审：中等 14） */
  private inflightSince = 0;
  /** 作废计数器：被作废的那一笔回来时认不出自己，不许再改状态 */
  private writeEpoch = 0;
  private saveTimer: TimerHandle | null = null;
  private verifyTimer: TimerHandle | null = null;
  private lastChangeAt = 0;
  private stopped = false;
  /** 被摘下来了（节点被删/换落点）：对象还在，但它一律报"存不下去"——见 `detach()` */
  private detached = false;

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
    // 回滚/排版清理会"换内容不换实例"：在飞的那一笔写的是旧内容，作废它，
    // 免得它回来时把刚建立的基准改回旧指纹。
    this.abandonInFlight();
    this.text = body;
    this.rev = 0;
    this.savedRev = 0;
    this.savedFingerprint = ack.fingerprint;
    this.charCount = ack.char_count;
    this.charsNoPunct = ack.chars_no_punct;
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

  /**
   * 立刻落盘（失焦 / 关窗前用，不等防抖）。
   *
   * **契约：返回时手上这一版必须已经落进库里；落不下去就抛。**
   * 这两件事原来都是假的（2026-09-15 代码质量评审：严重 1）：
   *
   * - 有写在飞时，这里只把那一笔**旧的**写等回来就返回，期间的击键仍是 pending（随后
   *   切章会把防抖定时器取消）——于是"先落盘再切章 / 删章 / 删书 / 回滚 / 换库 / 搬家"
   *   这六道守卫，全都拦不住最后那一段输入；
   * - 写失败时 `saveNow` 只把状态置成 error 就 `return`，这里照样正常 resolve——
   *   于是六道守卫的 `try/catch` 永远不触发，动作照旧执行，没存上的内容被顶掉。
   *
   * 现在：排到干净为止（最多 `FLUSH_ROUNDS` 笔，避免写不进去时空转），还脏就抛错。
   * 抛错是刻意的——**"存不下去就不许走"是这条承诺的实现方式**，调用方必须接住并拦住动作。
   *
   * 而且**每笔都有期限**：一次永不返回的 IPC 不能把这里永久挂住（评审：中等 14）——
   * 超过 `stuckMs` 还不回来就作废那一笔并按"卡住"报错，调用方与关窗闸门都能照常走到
   * "拦住 + 弹对话框"那一步。
   */
  async flush(): Promise<void> {
    this.cancelSaveTimer();
    if (this.detached) {
      // 已经摘下来了：没有任何东西能把手上这一版写进去——明确失败，闸门据此拦人
      throw new Error(this.detail || t("autosave.detached"));
    }
    for (let round = 0; round < FLUSH_ROUNDS && this.rev > this.savedRev; round += 1) {
      if (await this.saveWithinDeadline()) break; // 挂住了：别再空等下一轮
    }
    if (this.rev > this.savedRev) {
      // 还在脏着：把失败说清楚（原因多半已经在 detail 里了）
      throw new Error(this.detail || t("autosave.save_failed", { detail: "" }));
    }
  }

  /** 立刻做一次读回校验（测试与"我现在就想确认一下"用）。 */
  async verifyNow(): Promise<void> {
    if (this.stopped) return;

    // ⓪ 在飞的那一笔一直不回来（IPC/核心挂住）：以前这里直接 return，于是读回校验、
    //    卡住抢救全部停摆——"不返回"比"报错"更没有出路（评审：中等 14）。
    if (this.inFlight) {
      if (this.now() - this.inflightSince <= this.stuckMs) return;
      this.abandonInFlight();
      this.setStatus("error", t("autosave.save_stuck", { ms: this.stuckMs }));
      await this.rescue("stuck");
      return;
    }

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
      this.setStatus("error", t("autosave.verify_failed", { detail: messageOf(e) }));
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

  /**
   * **摘下来**：停掉它，但留着这个对象——它从此一律报"存不下去"。
   *
   * 为什么与 `dispose()` 分开：`dispose()` 之后通常会把对象丢掉（`autosave.value = null`），
   * 而"编辑器还挂着、落盘控制器却没了"是个**静默危险态**：打字没人接、状态栏还停在上一章的
   * saved、退出闸门见 null 直接放行（2026-09-15 代码质量评审：严重 6）。
   * 删掉当前章之后就该用这个：红字亮在状态栏、退出闸门拦得住人，
   * 作者不会在"看起来一切正常"里丢掉刚写的字。
   */
  detach(): void {
    this.detached = true;
    this.setStatus("error", t("autosave.detached"));
    this.dispose();
  }

  state(): AutosaveState {
    return {
      status: this.status,
      detail: this.detail,
      char_count: this.charCount,
      chars_no_punct: this.charsNoPunct,
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
    const epoch = this.writeEpoch;
    this.setStatus("saving");

    const run = async (): Promise<void> => {
      try {
        const ack = await this.transport.save(this.node_id, text);
        if (epoch !== this.writeEpoch) return; // 这一笔已被作废（挂住 → 抢救）：不许再改状态
        this.savedFingerprint = ack.fingerprint;
        this.savedRev = rev;
        this.charCount = ack.char_count;
        this.charsNoPunct = ack.chars_no_punct;
        this.wordCount = ack.word_count;
      } catch (e) {
        if (epoch !== this.writeEpoch) return;
        this.setStatus("error", t("autosave.save_failed", { detail: messageOf(e) }));
        this.armSave(); // 还脏着：过一会儿再试
        return;
      } finally {
        if (epoch === this.writeEpoch) this.inFlight = null;
      }
      if (epoch !== this.writeEpoch) return;
      if (rev === this.rev) {
        this.setStatus("saved");
      } else {
        // 写回期间作者又改了：继续排队，不能把最后这几次击键留在内存里
        this.setStatus("pending");
        this.armSave();
      }
    };

    this.inflightSince = this.now();
    this.inFlight = run();
    return this.inFlight;
  }

  /**
   * 落一笔盘，但**不无限等**：在飞的那一笔超过 `stuckMs` 还不回来就作废它。
   *
   * 返回 true = 这一笔挂住了（已经作废并按卡住报了错）；false = 有结果（成功或失败）。
   */
  private async saveWithinDeadline(): Promise<boolean> {
    const settled = await this.settledWithin(this.saveNow(), this.stuckMs);
    if (settled) return false;
    this.abandonInFlight();
    this.setStatus("error", t("autosave.save_stuck", { ms: this.stuckMs }));
    return true;
  }

  /** 作废在飞的那一笔：它若在抢救之后才回来，不许再把状态改回去（epoch 守卫）。 */
  private abandonInFlight(): void {
    if (this.inFlight === null) return;
    this.writeEpoch += 1;
    this.inFlight = null;
  }

  /** `promise` 在 `ms` 内有没有了结（用注入的计时器，测试里可以拨快）。 */
  private settledWithin(promise: Promise<void>, ms: number): Promise<boolean> {
    return new Promise<boolean>((resolve) => {
      let settled = false;
      const timer = this.schedule(() => {
        if (settled) return;
        settled = true;
        resolve(false);
      }, ms);
      const done = (): void => {
        if (settled) return;
        settled = true;
        this.cancel(timer);
        resolve(true);
      };
      promise.then(done, done);
    });
  }

  /** 抢救：手上这份先留快照，再逼着库回到这一版。 */
  private async rescue(reason: "stuck" | "desync"): Promise<void> {
    const text = this.text;
    const rev = this.rev;
    const label = t(reason === "stuck" ? "autosave.reason_stuck" : "autosave.reason_desync");
    this.incident = label;
    this.setStatus("desync", t("autosave.rescued", { label }));
    try {
      const ack = await this.transport.emergency(this.node_id, text, reason);
      this.savedFingerprint = ack.fingerprint;
      this.savedRev = rev;
      this.charCount = ack.char_count;
      this.charsNoPunct = ack.chars_no_punct;
      this.wordCount = ack.word_count;
      if (rev === this.rev) {
        this.setStatus("saved", t("autosave.recovered", { label }));
      } else {
        this.setStatus("pending");
        this.armSave();
      }
    } catch (e) {
      this.setStatus("error", t("autosave.rescue_failed", { detail: messageOf(e) }));
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
