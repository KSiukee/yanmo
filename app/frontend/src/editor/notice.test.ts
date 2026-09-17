// 启动交代的判定：**卡死优先于崩溃**，以及"没什么可说就别说话"。
//
// 这一条守的是一个具体的假警报：自动重载回来的那一趟，会话标记仍是"未正常退出"，
// 若不把"卡过"排在前面，作者每次复活都会看到"上次没有正常退出"——而那句话
// 是他唯一的崩溃线索，误报几次就没人信了。

import { test } from "node:test";
import assert from "node:assert/strict";

import { startupNotice } from "./notice.ts";
import type { SessionNotice } from "../api/core.ts";

function notice(patch: Partial<SessionNotice> = {}): SessionNotice {
  return {
    unclean: false,
    last_node_id: null,
    last_seen_at: null,
    revive: null,
    watchdog: "alive",
    revives: 0,
    ...patch,
  };
}

const freeze = { at: 1_700_000_000_000, node_id: 7, fingerprint: "abc", attempt: 1, gave_up: false };

test("干净退出、也没卡过：什么都不说", () => {
  assert.equal(startupNotice(notice()), null);
});

test("被杀 / 崩溃：说崩溃那一句（带上最后活动时间）", () => {
  const text = startupNotice(notice({ unclean: true, last_seen_at: 1_700_000_000_000 }), () => "昨天 20:01");
  assert.match(text ?? "", /昨天 20:01/);
  assert.match(text ?? "", /没有正常退出/);
});

test("时间读不出来也不许留空档", () => {
  const text = startupNotice(notice({ unclean: true }), () => "不该被用到");
  assert.match(text ?? "", /时间未知/);
});

test("自动重载回来的那一趟：说卡死那一句，**不说**假警报", () => {
  const text = startupNotice(notice({ unclean: true, revive: { ...freeze } }), () => "不该被用到");
  assert.match(text ?? "", /自动重载/);
  assert.doesNotMatch(text ?? "", /没有正常退出/, "重载期间标记仍是未正常退出，不能照念");
});

test("重载没救回来：把次数说清楚", () => {
  const text = startupNotice(notice({ unclean: true, revive: { ...freeze, attempt: 3, gave_up: true } }));
  assert.match(text ?? "", /3/);
  assert.match(text ?? "", /关掉重开/);
});
