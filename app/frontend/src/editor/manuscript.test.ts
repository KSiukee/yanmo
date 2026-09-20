// 稿件台纯规则的验收：**看板列序、拖拽合法性、未完成散篇、倒计时与日期**。
//
// 这一份不碰界面也不碰核心（真命令由会话层注入），所以它能钉住"设计里的那几条口径"：
// 七列都在、已发出不去、进「已投」不能空手拖、未完成只在设过目标的稿子里找。

import { test } from "node:test";
import assert from "node:assert/strict";

import type { ManuscriptRow, ManuscriptStatus } from "../api/manuscript";
import {
  BOARD_COLUMNS,
  canDrop,
  dateInputToMs,
  daysUntil,
  defaultCooldownUntil,
  groupByStatus,
  isUnfinished,
  msToDateInput,
  needsTarget,
  nextStatuses,
  overdue,
  overdueRows,
  remainingWords,
  unfinishedRows,
  untargetedRows,
} from "./manuscript.ts";

const DAY = 86_400_000;

function row(partial: Partial<ManuscriptRow> = {}): ManuscriptRow {
  return {
    id: 1,
    kind: "article",
    title: "一篇",
    status: "draft",
    word_count: 100,
    char_count: 100,
    chars_no_punct: 100,
    target_words: null,
    submissions: 0,
    active_venue: null,
    active_submitted_at: null,
    active_cooldown_until: null,
    last_venue: null,
    updated_at: 0,
    ...partial,
  };
}

test("看板就是那七列，顺序照核心", () => {
  assert.deepEqual(BOARD_COLUMNS, [
    "draft",
    "finished",
    "ready",
    "submitted",
    "accepted",
    "rejected",
    "published",
  ]);
});

test("已发是唯一出不去的态；被拒一定出得去（重出路径）", () => {
  for (const column of BOARD_COLUMNS) {
    const out = nextStatuses(column);
    assert.equal(out.length === 0, column === "published", `${column} 的出边不对`);
  }
  assert.deepEqual(nextStatuses("rejected"), ["draft", "finished", "ready"]);
});

test("进「已投」不是一个能拖过去的状态（它要记一次投稿）", () => {
  for (const column of BOARD_COLUMNS) {
    assert.equal(nextStatuses(column).includes("submitted"), false, `${column} 不该直接拖到已投`);
  }
  assert.equal(canDrop("draft", "submitted"), false);
});

test("拖拽合法性：能落的落得下，说不通的不让落", () => {
  assert.equal(canDrop("draft", "ready"), true);
  assert.equal(canDrop("finished", "draft"), true);
  assert.equal(canDrop("submitted", "ready"), true, "撤回");
  assert.equal(canDrop("ready", "accepted"), true, "撤稿/超期之后晚来的回信");
  assert.equal(canDrop("accepted", "published"), true);
  assert.equal(canDrop("published", "draft"), false);
  assert.equal(canDrop("draft", "accepted"), false);
});

test("未完成散篇：只有设过完稿目标的才判得了", () => {
  assert.equal(isUnfinished(row({ status: "draft", target_words: 1000, char_count: 400 })), true);
  assert.equal(isUnfinished(row({ status: "finished", target_words: 1000, char_count: 400 })), true);
  assert.equal(
    isUnfinished(row({ status: "draft", target_words: null, char_count: 10 })),
    false,
    "没定目标就不猜",
  );
  assert.equal(
    isUnfinished(row({ status: "accepted", target_words: 1000, char_count: 10 })),
    false,
    "都过稿了，不叫欠着",
  );
  assert.equal(
    isUnfinished(row({ status: "draft", target_words: 1000, char_count: 1000 })),
    false,
    "写够了就不是未完成",
  );
});

test("开了坑还没定目标的：单列一类，不算未完成", () => {
  assert.equal(needsTarget(row({ status: "draft", target_words: null })), true);
  assert.equal(needsTarget(row({ status: "finished", target_words: null })), true);
  assert.equal(needsTarget(row({ status: "draft", target_words: 100 })), false);
  assert.equal(needsTarget(row({ status: "published", target_words: null })), false);
});

test("还差多少字：写够了给 0，不会给负数", () => {
  assert.equal(remainingWords(row({ target_words: 1000, char_count: 400 })), 600);
  assert.equal(remainingWords(row({ target_words: 1000, char_count: 1200 })), 0);
  assert.equal(remainingWords(row({ target_words: null, char_count: 10 })), 0);
});

test("未完成散篇先提醒最久没动的那些", () => {
  const stale = row({ id: 1, target_words: 1000, char_count: 10, updated_at: 100 });
  const fresh = row({ id: 2, target_words: 1000, char_count: 10, updated_at: 900 });
  const done = row({ id: 3, target_words: 10, char_count: 10, updated_at: 1 });
  const list = unfinishedRows([fresh, done, stale]);
  assert.deepEqual(
    list.map((item) => item.id),
    [1, 2],
  );
  assert.deepEqual(
    untargetedRows([row({ id: 9, updated_at: 5 }), row({ id: 8, updated_at: 1 })]).map(
      (item) => item.id,
    ),
    [8, 9],
  );
});

test("还有几天：向上取整，过了就是 0 或负数", () => {
  const now = 1_000_000_000;
  assert.equal(daysUntil(now + 1, now), 1, "还剩一点也算还有今天这一天");
  assert.equal(daysUntil(now + DAY, now), 1);
  assert.equal(daysUntil(now + DAY + 1, now), 2);
  assert.equal(daysUntil(now, now), 0);
  assert.equal(daysUntil(now - DAY, now), -1);
});

test("超期只认「期限已过」：没定期限不算超期", () => {
  const now = 1_000_000_000;
  assert.equal(overdue(now - 1, now), true);
  assert.equal(overdue(now + 1, now), false);
  assert.equal(overdue(null, now), false);
});

test("超期未回的那几篇：只算定过期限、且期限已过的", () => {
  const now = 1_000_000_000;
  const stale = row({ id: 1, active_venue: "甲刊", active_cooldown_until: now - 1 });
  const waiting = row({ id: 2, active_venue: "乙刊", active_cooldown_until: now + 1 });
  const noDeadline = row({ id: 3, active_venue: "丙刊", active_cooldown_until: null });
  const idle = row({ id: 4 });
  assert.deepEqual(
    overdueRows([waiting, idle, stale, noDeadline], now).map((item) => item.id),
    [1],
  );
  assert.deepEqual(overdueRows([waiting, idle, noDeadline], now), []);
});

test("日期输入框与毫秒来回换算（本地日历日）", () => {
  assert.equal(msToDateInput(null), "");
  assert.equal(dateInputToMs(""), null);
  assert.equal(dateInputToMs("不是日期"), null);
  const noon = new Date(2026, 8, 20, 12, 0, 0).getTime();
  assert.equal(msToDateInput(noon), "2026-09-20");
  assert.equal(dateInputToMs("2026-09-20"), new Date(2026, 8, 20).getTime());
});

test("默认冷冻期：从投稿那天往后推核心给的天数", () => {
  assert.equal(defaultCooldownUntil(1_000, 90), 1_000 + 90 * DAY);
  assert.equal(defaultCooldownUntil(0, 0), 0);
});

test("按状态分列：七列都在（空列也占着，看板才是看板）", () => {
  const columns = groupByStatus([row({ id: 1, status: "ready" }), row({ id: 2, status: "ready" })]);
  const names = Object.keys(columns) as ManuscriptStatus[];
  assert.deepEqual(names, BOARD_COLUMNS);
  assert.equal(columns.ready.length, 2);
  assert.equal(columns.draft.length, 0);
});
