// 加密备份包那一格的界面状态：开关 / 口令 / 恢复码。
//
// 这里不测壳与核心（派生、加密、恢复码生成都有自己的验收），只测界面这一层最容易写错的三件事：
// 1. 恢复码拿到之后**必须作者确认过才收起**——那是唯一一次能抄下来的机会（包里取不回来）；
// 2. 本机已经有一把恢复码时**不重复给新码**（换掉它会让以前那些包失去第二把钥匙）；
// 3. 失败要如实报出来，而且 **busy 必须松开**（否则按钮永远点不动，只能重开界面）。

import { test } from "node:test";
import assert from "node:assert/strict";

import type { SealStatus } from "../api/seal.ts";
import { useSeal } from "./seal.ts";

function status(over: Partial<SealStatus> = {}): SealStatus {
  return { enabled: false, unlocked: false, has_recovery: false, ...over };
}

/**
 * 壳那一份状态的替身：动作会改它，之后读回来就是"新的现状"
 *（界面显示的一律是读回来的事实，不是自己推断的）。
 */
function harness(options: { code?: string | null; fail?: "enable" | "status" } = {}) {
  const calls = { enabled: 0, disabled: 0, unlocks: 0, locks: 0 };
  let current = status();
  const transport = {
    status: async () => {
      if (options.fail === "status") throw new Error("backup.seal_locked");
      return current;
    },
    enable: async () => {
      if (options.fail === "enable") throw new Error("backup.seal_passphrase_empty");
      calls.enabled += 1;
      current = status({ enabled: true, unlocked: true, has_recovery: true });
      // 没给 code 就是"本机还没有过恢复码"那一档
      return options.code === undefined ? "ABCD-EFGH-IJKL-MNPQ" : options.code;
    },
    disable: async () => {
      calls.disabled += 1;
      current = status();
      return current;
    },
    unlock: async () => {
      calls.unlocks += 1;
      current = status({ enabled: true, unlocked: true, has_recovery: true });
      return current;
    },
    lock: async () => {
      calls.locks += 1;
      current = status({ enabled: true, unlocked: false, has_recovery: true });
    },
  };
  const seal = useSeal({ transport });
  return { seal, calls };
}

test("开启拿到恢复码：亮出卡片，**确认之前谁都不许收起**，确认之后才收", async () => {
  const { seal } = harness();
  await seal.enable("一句口令");
  assert.equal(seal.status.value?.enabled, true);
  assert.equal(seal.status.value?.unlocked, true);
  assert.equal(seal.showRecovery.value, true, "拿到码就要把卡片亮出来");
  assert.equal(seal.recoveryCode.value, "ABCD-EFGH-IJKL-MNPQ");

  // 顺手锁上 / 再读一次现状都不该把它收走：只有作者说"抄好了"才收
  await seal.lock();
  await seal.load();
  assert.equal(seal.showRecovery.value, true, "除了确认，谁都不许收起这张卡片");

  seal.confirmRecovery();
  assert.equal(seal.showRecovery.value, false);
  assert.equal(seal.recoveryCode.value, "", "收起之后那串码不再留在界面上");
});

test("本机已经有一把恢复码（enable 返回 null）：不亮卡片，只说一句「已有一把」", async () => {
  const { seal } = harness({ code: null });
  await seal.enable("另一句口令");
  assert.equal(seal.showRecovery.value, false, "没给新码就不该亮卡片");
  assert.equal(seal.recoveryCode.value, "");
  assert.equal(seal.recoveryExists.value, true);
  assert.equal(seal.status.value?.enabled, true, "开关照样开了");
});

test("锁上之后现状里 unlocked 变 false（读回壳那一份，不自己推断）", async () => {
  const { seal, calls } = harness();
  await seal.unlock("一句口令");
  assert.equal(seal.status.value?.unlocked, true);
  await seal.lock();
  assert.equal(calls.locks, 1);
  assert.equal(seal.status.value?.unlocked, false);
  assert.equal(seal.error.value, null);
});

test("给口令不校验对错：记住就算数，对不对要等真去开一个包才知道", async () => {
  const { seal, calls } = harness();
  await seal.unlock("随便打的一句");
  assert.equal(calls.unlocks, 1);
  assert.equal(seal.error.value, null, "没去开包之前不该报错");
  assert.equal(seal.status.value?.unlocked, true);
});

test("传输抛错：error 有值、busy 复位（按钮不能就此卡住）", async () => {
  const { seal } = harness({ fail: "enable" });
  await seal.enable("一句口令");
  assert.match(seal.error.value ?? "", /seal_passphrase_empty/);
  assert.equal(seal.busy.value, false);
  assert.equal(seal.status.value, null, "没开成就不该显示成开着");
});

test("读现状失败：如实报一句，状态置空（别拿旧值当真）", async () => {
  const { seal } = harness({ fail: "status" });
  await seal.load();
  assert.equal(seal.status.value, null);
  assert.match(seal.error.value ?? "", /seal_locked/);
});
