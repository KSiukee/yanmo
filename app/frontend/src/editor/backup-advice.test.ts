// 「该用哪块盘」的验收：备份要放在**另一块**盘上，推荐规则必须说得清。
//
// 判据：
// 1. 没盘 / 只有数据所在盘 / 剩余空间太小 → 不推荐（别硬凑一块）；
// 2. 可移动盘优先（备份完能拔走），同类里剩余空间大的优先；
// 3. 数据所在盘**永远不推荐**——哪怕它是唯一一块、哪怕它可移动。

import { test } from "node:test";
import assert from "node:assert/strict";

import { MIN_FREE_BYTES, recommendVolume } from "./backup-advice.ts";
import type { BackupVolume } from "../api/core/backup";

const GB = 1024 ** 3;

// 测试要造「盘符」这种形状。下面四行是本文件里唯一出现盘符的地方，
// 都是测试数据、不是本机真实路径，所以逐行标注豁免。
const C = "C:\\"; // sanitize-allow: 测试数据里的盘符
const D = "D:\\"; // sanitize-allow: 测试数据里的盘符
const E = "E:\\"; // sanitize-allow: 测试数据里的盘符
const F = "F:\\"; // sanitize-allow: 测试数据里的盘符

/** 造一块盘（默认：固定盘、100 GB、空间充足、不是数据所在盘）。 */
function volume(root: string, overrides: Partial<BackupVolume> = {}): BackupVolume {
  return {
    root,
    label: "",
    volume_id: root,
    removable: false,
    free_bytes: 10 * GB,
    total_bytes: 100 * GB,
    holds_data: false,
    ...overrides,
  };
}

test("一块合适的盘都没有时不硬推荐", () => {
  assert.equal(recommendVolume([]), null);
  assert.equal(recommendVolume([volume(D, { holds_data: true })]), null, "数据所在盘不推荐");
  assert.equal(
    recommendVolume([volume(E, { free_bytes: MIN_FREE_BYTES - 1 })]),
    null,
    "剩余空间不够下限不推荐"
  );
});

test("可移动盘优先——哪怕固定盘更大", () => {
  const fixed = volume(D, { free_bytes: 900 * GB });
  const usb = volume(E, { removable: true, free_bytes: 8 * GB });
  const advice = recommendVolume([fixed, usb]);
  assert.equal(advice?.volume.root, E);
  assert.equal(advice?.reason, "removable");
});

test("同为可移动时，剩余空间大的优先", () => {
  const small = volume(E, { removable: true, free_bytes: 4 * GB });
  const big = volume(F, { removable: true, free_bytes: 40 * GB });
  assert.equal(recommendVolume([small, big])?.volume.root, F);
});

test("只有固定盘（空间够）也会推荐，理由如实说是「空间大」", () => {
  const advice = recommendVolume([volume(D)]);
  assert.equal(advice?.volume.root, D);
  assert.equal(advice?.reason, "roomy");
});

test("数据所在的那块盘即使可移动、空间最大也不选", () => {
  const data = volume(C, { removable: true, holds_data: true, free_bytes: 500 * GB });
  const other = volume(D, { free_bytes: 1 * GB });
  assert.equal(recommendVolume([data, other])?.volume.root, D);
});
