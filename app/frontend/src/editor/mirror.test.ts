// 镜像状态行的验收：**四种事实各说各的话，且不编**。
//
// 最要紧的一条是"没有报告 ≠ 已对上 0 份"：把"还没对过"说成"已对上"，作者就会以为
// 磁盘上已经有那份 .md 了——而这个功能的全部意义就是"磁盘上真的有一份"。

import { test } from "node:test";
import assert from "node:assert/strict";

import type { MirrorIssue, MirrorStatus } from "../api/mirror";
import { clockText, issueActions, issueKey, mirrorLine } from "./mirror.ts";

const base: MirrorStatus = {
  enabled: true,
  root: "mirror",
  last_sync_at: 0,
  files: 0,
  conflicts: 0,
  failed_works: 0,
  last_error: "",
};

test("没有报告时按「还没对过」说，不编一个 0 份", () => {
  assert.deepEqual(mirrorLine(null), { kind: "never" });
});

test("关着就是关着（不因为 last_sync_at 有值就说已对上）", () => {
  assert.deepEqual(mirrorLine({ ...base, enabled: false, last_sync_at: 123 }), { kind: "off" });
});

test("开着但还没对上过，如实说没对过", () => {
  assert.deepEqual(mirrorLine(base), { kind: "never" });
});

test("对上过就把份数与时刻一起报出来", () => {
  assert.deepEqual(mirrorLine({ ...base, last_sync_at: 1000, files: 42 }), {
    kind: "synced",
    files: 42,
    at: 1000,
  });
});

test("时刻按本地时分秒补零", () => {
  const at = new Date(2026, 0, 2, 3, 4, 5).getTime();
  assert.equal(clockText(at), "03:04:05");
});

const issue = (kind: string, node_id: number): MirrorIssue => ({
  kind,
  node_id,
  title: "第一章",
  relative_path: "长夜-1/001-第一卷/001-第一章.md",
});

test("清单的键认路径也认类型（同一个文件先后换过类型算两条）", () => {
  assert.equal(issueKey(issue("edited", 7)), "edited:长夜-1/001-第一卷/001-第一章.md");
  assert.notEqual(issueKey(issue("edited", 7)), issueKey(issue("untracked", 0)));
});

test("对得上账的才给「收进研墨 / 盖回去」，没账的只给看", () => {
  assert.deepEqual(issueActions(issue("edited", 7)), { adopt: true, overwrite: true });
  // 没账的文件：node_id 是 0，研墨不知道它是哪一章
  assert.deepEqual(issueActions(issue("untracked", 0)), { adopt: false, overwrite: false });
  // 有类型没节点（理论上不该出现）也当"只能看"，绝不拿 0 号节点去动文件
  assert.deepEqual(issueActions(issue("edited", 0)), { adopt: false, overwrite: false });
});
