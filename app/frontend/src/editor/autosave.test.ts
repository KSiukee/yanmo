// 边写边存的逻辑测试：**用假时钟把时间拨快**，不真的等下去。
//
// 覆盖的是"不丢稿"承诺背后的机制：防抖合并、落盘期间的改动不丢、卡住与不一致能被发现并抢救。
// 零依赖：Node 自带的测试运行器 + 类型剥离，`npm test` 就能跑。

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  Autosave,
  DEBOUNCE_RANGE,
  VERIFY_RANGE,
  type AutosaveTransport,
  type SaveAck,
} from "./autosave.ts";

/** 让所有已排队的微任务跑完。 */
const settle = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

type Timer = ReturnType<typeof setTimeout>;

/** 假时钟：`advance(ms)` 把到期的回调按时间顺序放出来。 */
class FakeClock {
  private t = 0;
  private seq = 1;
  private timers = new Map<number, { at: number; fn: () => void }>();

  now = () => this.t;

  schedule = (fn: () => void, ms: number): Timer => {
    const id = this.seq++;
    this.timers.set(id, { at: this.t + ms, fn });
    return id as unknown as Timer;
  };

  cancel = (handle: Timer): void => {
    this.timers.delete(handle as unknown as number);
  };

  async advance(ms: number): Promise<void> {
    const target = this.t + ms;
    for (;;) {
      let pick = -1;
      let pickAt = Number.POSITIVE_INFINITY;
      for (const [id, timer] of this.timers) {
        if (timer.at <= target && timer.at < pickAt) {
          pickAt = timer.at;
          pick = id;
        }
      }
      if (pick < 0) break;
      const timer = this.timers.get(pick)!;
      this.timers.delete(pick);
      this.t = timer.at;
      timer.fn();
      await settle();
    }
    this.t = target;
    await settle();
  }
}

/** 假核心：记住"库里"的正文与指纹，可以人为写失败、挂起、被外部改动。 */
function fakeCore(options: { manual?: boolean } = {}) {
  const calls: string[] = [];
  const gates: Array<() => void> = [];
  let storedBody = "";
  let storedFingerprint = "";
  let failing = false;

  const fp = (body: string) => `fp(${body})`;
  const ack = (body: string): SaveAck => ({
    char_count: body.length,
    chars_no_punct: body.length,
    word_count: body.length,
    fingerprint: fp(body),
  });

  const transport: AutosaveTransport = {
    async save(_node_id, body) {
      calls.push(`save:${body}`);
      if (options.manual) await new Promise<void>((resolve) => gates.push(resolve));
      if (failing) throw new Error("磁盘写入失败");
      storedBody = body;
      storedFingerprint = fp(body);
      return ack(body);
    },
    async fingerprint() {
      calls.push("fingerprint");
      return storedFingerprint;
    },
    async emergency(_node_id, body, reason) {
      calls.push(`emergency:${reason}:${body}`);
      storedBody = body;
      storedFingerprint = fp(body);
      return ack(body);
    },
  };

  return {
    transport,
    calls,
    gates,
    saves: () => calls.filter((c) => c.startsWith("save:")),
    alwaysFail: (value: boolean) => {
      failing = value;
    },
    setStored: (body: string) => {
      storedBody = body;
      storedFingerprint = fp(body);
    },
    setStoredFingerprint: (value: string) => {
      storedFingerprint = value;
    },
    stored: () => storedBody,
  };
}

function build(core: ReturnType<typeof fakeCore>, clock: FakeClock, overrides = {}) {
  return new Autosave({
    node_id: 7,
    transport: core.transport,
    now: clock.now,
    schedule: clock.schedule,
    cancel: clock.cancel,
    ...overrides,
  });
}

test("停笔才落盘：连续击键只写一次，写的是最后那一版", async () => {
  const clock = new FakeClock();
  const core = fakeCore();
  const autosave = build(core, clock);
  autosave.attach("", { char_count: 0, chars_no_punct: 0, word_count: 0, fingerprint: "" });

  autosave.changed("第一");
  await clock.advance(100);
  autosave.changed("第一句");
  await clock.advance(100);
  autosave.changed("第一句话");
  await clock.advance(300);

  assert.deepEqual(core.saves(), ["save:第一句话"]);
  assert.equal(autosave.state().status, "saved");
  assert.equal(autosave.state().char_count, 4);
});

test("落盘期间的新改动不会被这一次写吞掉", async () => {
  const clock = new FakeClock();
  const core = fakeCore({ manual: true });
  const autosave = build(core, clock);
  autosave.attach("", { char_count: 0, chars_no_punct: 0, word_count: 0, fingerprint: "" });

  autosave.changed("第一句");
  await clock.advance(250);
  assert.equal(autosave.state().status, "saving");

  autosave.changed("第一句，又补了一句");
  await clock.advance(50);
  core.gates.shift()!(); // 第一次写回来
  await settle();
  await clock.advance(250);
  core.gates.shift()!(); // 第二次写回来
  await settle();

  assert.deepEqual(core.saves(), ["save:第一句", "save:第一句，又补了一句"]);
  assert.equal(autosave.state().status, "saved");
  assert.equal(core.stored(), "第一句，又补了一句");
});

test("失焦立刻落盘，不等防抖", async () => {
  const clock = new FakeClock();
  const core = fakeCore();
  const autosave = build(core, clock);
  autosave.attach("", { char_count: 0, chars_no_punct: 0, word_count: 0, fingerprint: "" });

  autosave.changed("马上要切走了");
  await autosave.flush();

  assert.deepEqual(core.saves(), ["save:马上要切走了"]);
  assert.equal(autosave.state().status, "saved");
});

test("flush 要等到**最新一版**落盘：有写在飞时不是等那笔旧的就算完", async () => {
  // 失效模式 A（2026-09-15 代码质量评审：严重 1）：触发条件是"防抖窗口内落两次笔"——
  // 打字本来就是那个节奏；随后作者一点另一章（或 Ctrl+Alt+← / 删章），
  // flush 只把第一笔等回来就返回，第二笔仍是 pending，最后那一段输入永远没入库。
  const clock = new FakeClock();
  const core = fakeCore({ manual: true });
  const autosave = build(core, clock);
  autosave.attach("", { char_count: 0, chars_no_punct: 0, word_count: 0, fingerprint: "" });

  autosave.changed("第一句");
  await clock.advance(250); // 第一笔进入在飞态（还没回来）
  assert.equal(autosave.state().status, "saving");

  autosave.changed("第一句，又补了一句");
  const flushing = autosave.flush(); // ← 在飞还没落地时要求"立刻落盘"

  core.gates.shift()!(); // 放行第一笔
  await settle();
  core.gates.shift()!(); // flush 必须继续排第二笔（写的是最新那版）
  await flushing;

  assert.equal(core.stored(), "第一句，又补了一句", "flush 返回时库里的必须是最新一版");
  assert.equal(autosave.state().status, "saved");
});

test("存不下去时 flush 必须报失败（六道「先落盘再动手」的守卫靠它拦住动作）", async () => {  // 失效模式 B（评审：严重 1）：以前 saveNow 失败只置 error 就 return，flush 照常 resolve，
  // 于是切章/删章/删书/回滚/换库/搬家那六处 `try { await flush() } catch` 永不触发——
  // 动作照旧执行，没存上的内容被顶掉。
  const clock = new FakeClock();
  const core = fakeCore();
  const autosave = build(core, clock);
  autosave.attach("", { char_count: 0, chars_no_punct: 0, word_count: 0, fingerprint: "" });
  core.alwaysFail(true);

  autosave.changed("写不进去的一版");
  await assert.rejects(() => autosave.flush(), /保存失败/, "落不下去必须抛，不能悄悄返回");
  assert.equal(autosave.state().status, "error");
  assert.notEqual(core.stored(), "写不进去的一版", "库里确实没写进去（这是测试的前提）");
});

test("一直存不进去：先报错，超时后强制抢救", async () => {
  const clock = new FakeClock();
  const core = fakeCore();
  const autosave = build(core, clock);
  autosave.attach("", { char_count: 0, chars_no_punct: 0, word_count: 0, fingerprint: "" });
  core.alwaysFail(true);

  autosave.changed("写不进去的一版");
  await clock.advance(250);
  assert.equal(autosave.state().status, "error");
  assert.match(autosave.state().detail, /保存失败/);

  await clock.advance(1600); // 超过卡住阈值
  await autosave.verifyNow();

  assert.ok(
    core.calls.some((c) => c.startsWith("emergency:stuck:")),
    `卡住时必须抢救，实际调用：${core.calls.join(" | ")}`,
  );
  assert.equal(autosave.state().incident, "落盘长时间没有完成");
  assert.equal(core.stored(), "写不进去的一版", "抢救后库里应当是手上这一版");
});

test("读回校验发现库里的正文变了：立刻抢救并留痕", async () => {
  const clock = new FakeClock();
  const core = fakeCore();
  const autosave = build(core, clock);
  core.setStored("手上这一版正文");
  autosave.attach("手上这一版正文", { char_count: 7, chars_no_punct: 7, word_count: 7, fingerprint: "fp(手上这一版正文)" });

  // 库里被改成了别的东西（外部改动 / 写入丢失）
  core.setStoredFingerprint("fp(别人写的)");
  await clock.advance(autosave.verifyMs);

  assert.ok(
    core.calls.includes("emergency:desync:手上这一版正文"),
    `不一致时必须抢救，实际调用：${core.calls.join(" | ")}`,
  );
  assert.equal(autosave.state().incident, "库里的正文与手上这份不一致");
  assert.equal(core.stored(), "手上这一版正文");
  assert.equal(autosave.state().status, "saved");
  assert.match(autosave.state().detail, /已恢复/);
});

test("一切正常时不误报，也不多写", async () => {
  const clock = new FakeClock();
  const core = fakeCore();
  const autosave = build(core, clock);
  core.setStored("正文没变");
  autosave.attach("正文没变", { char_count: 4, chars_no_punct: 4, word_count: 4, fingerprint: "fp(正文没变)" });

  await clock.advance(VERIFY_RANGE.max);
  await clock.advance(VERIFY_RANGE.max);

  assert.equal(core.calls.some((c) => c.startsWith("emergency")), false);
  assert.equal(core.saves().length, 0, "没有改动就不该产生任何落盘");
  assert.equal(autosave.state().status, "saved");
  assert.equal(autosave.state().incident, null);
  assert.ok(core.calls.filter((c) => c === "fingerprint").length >= 2, "每隔几秒核一次");
});

test("关掉之后彻底安静", async () => {
  const clock = new FakeClock();
  const core = fakeCore();
  const autosave = build(core, clock);
  autosave.attach("", { char_count: 0, chars_no_punct: 0, word_count: 0, fingerprint: "" });

  autosave.changed("还没落盘就关了");
  autosave.dispose();
  await clock.advance(10_000);

  assert.deepEqual(core.calls, []);
});

test("防抖与校验间隔被夹在任务口径内", () => {
  const clock = new FakeClock();
  const core = fakeCore();

  const tooFast = build(core, clock, { debounceMs: 1, verifyMs: 100 });
  assert.equal(tooFast.debounceMs, DEBOUNCE_RANGE.min);
  assert.equal(tooFast.verifyMs, VERIFY_RANGE.min);
  tooFast.dispose();

  const tooSlow = build(core, clock, { debounceMs: 9999, verifyMs: 999_999 });
  assert.equal(tooSlow.debounceMs, DEBOUNCE_RANGE.max);
  assert.equal(tooSlow.verifyMs, VERIFY_RANGE.max);
  tooSlow.dispose();
});

test("节点被删后「摘下来」：状态亮红字、flush 一律失败、退出闸门因此拦得住人", async () => {
  // 失效模式（2026-09-15 代码质量评审：严重 6）：删掉正在写的那一支之后，
  // 老写法是 `dispose()` + `autosave.value = null`。若随后的换落点失败，控制器永久为 null——
  // 编辑器照常能打字、状态栏还停在上一章的 saved、退出闸门见 null 直接放行。
  // 现在：对象留着（摘下来），它自己把话说清楚。
  const clock = new FakeClock();
  const core = fakeCore();
  const autosave = build(core, clock);
  autosave.attach("", { char_count: 0, chars_no_punct: 0, word_count: 0, fingerprint: "" });

  autosave.detach();

  assert.equal(autosave.state().status, "error", "状态栏必须亮出来，不能停在 saved");
  assert.match(autosave.state().detail, /已被删除|存不进去/);

  // 摘下来之后写下的字不会往那个已删节点写（写了也是白写），但**退出时会被拦住**
  autosave.changed("摘下来之后写的字");
  await clock.advance(500);
  assert.deepEqual(core.saves(), [], "不该再往一个已经删掉的节点落盘");
  await assert.rejects(
    () => autosave.flush(),
    /已被删除|存不进去/,
    "摘下来之后 flush 必须失败——闸门靠它拦住退出，否则新写的字无声消失"
  );
});

test("一笔落盘永远不回来（IPC 卡住）：不锁死状态机，超时后作废并抢救", async () => {
  // 失效模式（2026-09-15 代码质量评审：中等 14）：`verifyNow` 首句是 `if (this.inFlight) return`，
  // 于是 transport.save 永不 settle 时，读回校验与卡住抢救全部停摆、退出闸门永久 blocked。
  // 触发条件是"不返回"（通道异常/核心卡在锁上），比"报错"更没有出路。
  const clock = new FakeClock();
  const core = fakeCore({ manual: true });
  const autosave = build(core, clock);
  autosave.attach("", { char_count: 0, chars_no_punct: 0, word_count: 0, fingerprint: "" });

  autosave.changed("挂在半路的一版");
  await clock.advance(250);
  assert.equal(autosave.state().status, "saving");

  await clock.advance(1600); // 超过卡住阈值
  await autosave.verifyNow(); // ← 校验不再被"在飞"永久关掉

  assert.ok(
    core.calls.some((c) => c.startsWith("emergency:stuck:")),
    `在飞的那一笔超时也必须抢救，实际调用：${core.calls.join(" | ")}`
  );
  assert.equal(core.stored(), "挂在半路的一版");
  assert.equal(autosave.state().status, "saved");

  // 又写了一点：新的一笔在飞
  autosave.changed("挂在半路的一版，又补了一句");
  await clock.advance(250);
  assert.equal(autosave.state().status, "saving");

  // 那一笔被作废的旧写这时候才回来：不许改状态，也不许把新那一笔的锁清掉
  core.gates.shift()!();
  await settle();
  assert.equal(autosave.state().status, "saving", "被作废的那一笔回来时不许动状态");

  core.gates.shift()!();
  await settle();
  assert.equal(autosave.state().status, "saved");
  assert.equal(core.stored(), "挂在半路的一版，又补了一句");
});

test("一笔落盘永远不回来：flush 有期限，不会把关窗闸门永久锁死", async () => {
  // 同一条失效模式的另一面：flush 里 `await saveNow()` 会等那一笔永不返回的 Promise，
  // 于是六道守卫与关窗闸门一起挂住——连"存不下去"的对话框都弹不出来。
  const clock = new FakeClock();
  const core = fakeCore({ manual: true });
  const autosave = build(core, clock);
  autosave.attach("", { char_count: 0, chars_no_punct: 0, word_count: 0, fingerprint: "" });

  autosave.changed("等不回来的一版");
  // 先把"必须失败"的断言挂上（到期才拒绝，这里不能晚于下一次宏任务）
  const rejected = assert.rejects(() => autosave.flush(), /没有回应/, "flush 必须有限时间内失败，不能永久挂着");
  assert.equal(autosave.state().status, "saving");

  await clock.advance(1600);
  await rejected;
  assert.equal(autosave.state().status, "error");
});

