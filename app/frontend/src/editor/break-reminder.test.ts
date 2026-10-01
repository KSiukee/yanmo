// 用眼提醒的验收：**只在停下点说一句、同一轮不重复、关掉就一声不响**。
//
// 全部打的是纯逻辑那一半（`useBreakWatch`）：时钟、字数、今日字数、偏好都从外面喂，
// 所以这里不需要浏览器、也不需要会话——判据本身就是"喂进去什么、什么时候够阈值"。

import { test } from "node:test";
import assert from "node:assert/strict";

import { has, t } from "../locales/index.ts";
import { breakTipText, useBreakWatch, type BreakSettings } from "./break-reminder.ts";

/** 一整套假环境：时钟自己走，字数与今日字数由测试摆，偏好可以中途换。 */
function fake(overrides: Partial<BreakSettings> = {}) {
  let time = 1_000_000;
  let chars = 0;
  let today = 0;
  let settings: BreakSettings | null = { on: true, minutes: 45, words: 1500, ...overrides };

  const watch = useBreakWatch({
    settings: () => settings,
    chars: () => chars,
    todayWords: () => today,
    now: () => time,
  });

  return {
    watch,
    /** 时间往前走（静置就是它；不喂活动，所以不算"还在写"） */
    idle(minutes: number) {
      time += minutes * 60_000;
    },
    /**
     * 连着写 `minutes` 分钟：每 `everyMs` 毫秒落一次盘（字数一变＝一次活动）。
     *
     * 一次活动只把**到上一次活动为止的那一段**累加进去——这正是"只在有活动时累加"。
     */
    write(minutes: number, everyMs = 60_000) {
      const steps = Math.round((minutes * 60_000) / everyMs);
      for (let step = 0; step < steps; step += 1) {
        chars += 10;
        time += everyMs;
        watch.poke();
      }
    },
    setToday(value: number) {
      today = value;
    },
    setSettings(next: BreakSettings | null) {
      settings = next;
    },
  };
}

test("没到阈值不响：停下点评估了也说不出话", () => {
  const env = fake();
  env.write(44); // 差一分钟
  env.watch.evaluate();
  assert.equal(env.watch.tip.value, null, "没到阈值就不该开口");
});

test("到了阈值**只在停下点响一次**——写作途中一句都不说", () => {
  const env = fake();
  env.write(45);
  assert.equal(env.watch.tip.value, null, "写的时候一声不响（评估只在停下点）");

  env.watch.evaluate(); // 切章 / 切书 / 关窗前那一下
  assert.deepEqual(env.watch.tip.value, { kind: "minutes", minutes: 45 });
});

test("同一轮不重复：收掉之后到下一个停下点也不再说", () => {
  const env = fake();
  env.write(50);
  env.watch.evaluate();
  assert.notEqual(env.watch.tip.value, null);

  env.watch.dismiss();
  assert.equal(env.watch.tip.value, null, "「知道了」就是把条收掉");
  env.watch.evaluate();
  env.write(2); // 还接着写
  env.watch.evaluate();
  assert.equal(env.watch.tip.value, null, "同一轮里再评估多少次都一样");
});

test("静置够久 ⇒ 计数清零，下一轮重新算", () => {
  const env = fake();
  env.write(45);
  env.idle(10); // 停手十分钟：这一轮算过去了

  env.watch.evaluate();
  assert.equal(env.watch.tip.value, null, "静置过了就不该拿旧账开口");

  env.write(46); // 又从零开始连着写（45 分钟前那一轮已经翻篇了）
  env.watch.evaluate();
  assert.deepEqual(
    env.watch.tip.value,
    { kind: "minutes", minutes: 46 },
    "重新算才够阈值，报的是这一轮真写了多久",
  );
});

test("★关掉 = 一声不响：不评估、不显示、也不记账", () => {
  const env = fake({ on: false });
  env.write(60);
  env.watch.evaluate();
  assert.equal(env.watch.tip.value, null, "关着就什么都别说");

  // 再打开时那一轮并没有被悄悄"用掉"——该说的时候照样说
  env.setSettings({ on: true, minutes: 45, words: 1500 });
  env.watch.evaluate();
  assert.deepEqual(env.watch.tip.value, { kind: "minutes", minutes: 60 });
});

test("读不回来也不响（不拿猜出来的阈值开口）", () => {
  const env = fake();
  env.setSettings(null);
  env.write(90);
  env.watch.evaluate();
  assert.equal(env.watch.tip.value, null);
});

test("两条判据各自记账、互不覆盖", () => {
  const env = fake();
  env.write(45);
  env.setToday(2000); // 两条都到点了

  env.watch.evaluate();
  assert.deepEqual(env.watch.tip.value, { kind: "minutes", minutes: 45 }, "先说着了多久");
  env.watch.dismiss();

  env.watch.evaluate();
  assert.deepEqual(env.watch.tip.value, { kind: "words", words: 2000 }, "再说今天写了多少");
  env.watch.dismiss();

  env.watch.evaluate();
  assert.equal(env.watch.tip.value, null, "两条都说过了，这一轮到此为止");
});

test("字数那条按**今日累计**判——不是按本次这一章写了多少", () => {
  const env = fake();
  env.write(1); // 这一轮几乎没写
  env.setToday(1600); // 但今天已经写够了
  env.watch.evaluate();
  assert.deepEqual(env.watch.tip.value, { kind: "words", words: 1600 });
});

test("今日字数掉回阈值以下（换了一天 / 删了字）⇒ 重新算", () => {
  const env = fake();
  env.setToday(2000);
  env.watch.evaluate();
  assert.deepEqual(env.watch.tip.value, { kind: "words", words: 2000 });
  env.watch.dismiss();

  env.setToday(100); // 新的一天
  env.watch.evaluate();
  assert.equal(env.watch.tip.value, null);

  env.setToday(1500); // 新的一天又写到了
  env.watch.evaluate();
  assert.deepEqual(env.watch.tip.value, { kind: "words", words: 1500 });
});

test("条上那一句照字典来（键名英文，句子在 locales 里）", () => {
  assert.equal(
    breakTipText({ kind: "minutes", minutes: 45 }),
    "你连着写了 45 分钟了——起来走两步，眼睛歇一会儿。",
  );
  assert.equal(
    breakTipText({ kind: "words", words: 1600 }),
    "今天写了 1600 字了。要不要就此收个尾？",
  );
  assert.equal(breakTipText(null), "", "没有那一句就什么都不显示");
  assert.ok(has("break.dismiss"), "关闭按钮那句话在字典里");
  assert.equal(t("break.dismiss"), "知道了");
});

// ── 章长那条（同一条出口上的第二个内容） ──────────────────────────────────
//
// 判据与记账在 `editor/chapter-length.ts`（那一份有自己的测试）；这里只管**出口**：
// 它必须在停下点、由接线层喂进来，而且不抢已经在说的那一句。

/** 章长那句话要念的事实（判据在核心，这里只喂结果）。 */
const CHAPTER = {
  work_id: 1,
  node_id: 3,
  chars: 4700,
  median: 2000,
  times: 2.4,
  fingerprint: "chapter-long:3",
};

test("★章长那句话只有 offerChapter 说得出来：写作途中、评估都不带它出来", () => {
  const env = fake();
  env.write(60); // 用眼那两条都到点了
  env.setToday(2000);
  assert.equal(env.watch.tip.value, null, "写的时候一声不响（章长那条更没有入口）");

  env.watch.evaluate();
  assert.equal(env.watch.tip.value?.kind, "minutes", "停下点先说的是用眼提醒（它排在前头）");
  env.watch.dismiss();
  env.watch.evaluate();
  assert.equal(env.watch.tip.value?.kind, "words", "还是用眼那条");
  env.watch.dismiss();

  env.watch.offerChapter(CHAPTER);
  assert.deepEqual(env.watch.tip.value, { kind: "chapter", ...CHAPTER }, "到这里章长那句才说得出来");
  assert.equal(
    breakTipText(env.watch.tip.value),
    "这一章 4700 字，这本书中位 2000 字（2.4 倍）——是故意的吗？",
    "念的是事实，末尾是问句",
  );
});

test("条上已经有一条时，章长那条不抢自己的话头", () => {
  const env = fake();
  env.write(45);
  env.watch.evaluate();
  assert.equal(env.watch.tip.value?.kind, "minutes");

  env.watch.offerChapter(CHAPTER);
  assert.equal(env.watch.tip.value?.kind, "minutes", "已经在说的那句不被顶掉");
});

test("「知道了」只关这一次：同一个停下点还超长就再提一次", () => {
  const env = fake();
  env.watch.offerChapter(CHAPTER);
  assert.equal(env.watch.tip.value?.kind, "chapter");

  env.watch.dismiss();
  assert.equal(env.watch.tip.value, null, "收掉就走");

  env.watch.offerChapter(CHAPTER);
  assert.equal(env.watch.tip.value?.kind, "chapter", "没记账 ⇒ 下次停下来还说得出来");
});

test("章长那条没有可说的（null）时，条上什么都不变", () => {
  const env = fake();
  env.watch.offerChapter(null);
  assert.equal(env.watch.tip.value, null);
});
