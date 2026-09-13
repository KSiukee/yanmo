// 「稿子放在哪」的界面这一层：**只测"什么时候叫它"与"叫不叫得动"**。
//
// 核心自己有验收（位置记录、老位置认领、复制与核对都在 yanmo-core/tests/location.rs）；
// 这里守三条最容易写错的界面规矩：
// 1. 首启只在"壳说这是第一次用"的时候弹（老作者升上来不该看见"首次使用"）；
// 2. 搬家前**必须先落盘**（beforeMove），落不下就不该搬；
// 3. 没选中新位置的时候，搬不动（不能拿上一次的选择去搬）。

import { test } from "node:test";
import assert from "node:assert/strict";

import type { LocationInfo, PickedDir, RelocationReport } from "../api/core.ts";
import { useLocation } from "./location.ts";

function info(over: Partial<LocationInfo> = {}): LocationInfo {
  return {
    path: "文档/研墨",
    pointer: "系统数据目录/app.yanmo.desktop/yanmo-location.txt",
    first_run: false,
    suggested_path: "文档/研墨",
    suggested_reason: "documents",
    ...over,
  };
}

function picked(path = "别处的稿子"): PickedDir {
  return { path, risks: [], entries: 0 };
}

const report: RelocationReport = { path: "别处的稿子", files: 9, bytes: 4096 };

/** 一套默认"都能成"的替身；每个测试只改它关心的那一个。 */
function harness(over: Partial<Parameters<typeof useLocation>[0]["transport"]> = {}) {
  const calls: string[] = [];
  const state = useLocation({
    transport: {
      info: async () => info(),
      pick: async () => picked(),
      confirm: async () => {
        calls.push("confirm");
      },
      move: async () => {
        calls.push("move");
        return report;
      },
      cancel: async () => {
        calls.push("cancel");
      },
      ...over,
    },
    beforeMove: async () => {
      calls.push("flush");
    },
  });
  return { state, calls };
}

test("首启只在那一次弹出来，而且推荐位置与理由都如实带过来", async () => {
  const first = harness({ info: async () => info({ first_run: true }) });
  assert.equal(first.state.visible.value, false, "没读过之前不该弹");
  await first.state.load();
  assert.equal(first.state.visible.value, true, "壳说是第一次用就该引导");
  assert.equal(first.state.info.value?.suggested_reason, "documents");

  const veteran = harness();
  await veteran.state.load();
  assert.equal(veteran.state.visible.value, false, "老作者不该看见'首次使用'");
});

test("首启选「就用这里」= 把位置记下来并收起引导", async () => {
  const { state, calls } = harness({ info: async () => info({ first_run: true }) });
  await state.load();
  await state.useCurrent();
  assert.deepEqual(calls, ["confirm"]);
  assert.equal(state.visible.value, false, "确认过就不该再挡着");
});

test("搬家前一定先落盘，落不下就不搬", async () => {
  const { state, calls } = harness();
  await state.load();
  await state.pickDir("选择文件夹");

  // 落盘失败：搬这个动作**根本不该发生**
  const blocked = useLocation({
    transport: {
      info: async () => info(),
      pick: async () => picked(),
      confirm: async () => {},
      move: async () => {
        calls.push("搬了");
        return report;
      },
      cancel: async () => {},
    },
    beforeMove: async () => {
      throw new Error("存不下去");
    },
    onError: (message) => calls.push(`错:${message}`),
  });
  await blocked.load();
  await blocked.pickDir("选择文件夹");
  await blocked.confirmMove();
  assert.equal(calls.includes("搬了"), false, "手上这一章没落盘就搬家＝丢字");
  assert.ok(
    calls.some((entry) => entry.startsWith("错:")),
    "失败要如实报出来，不能装作搬成功",
  );
});

test("没选位置时搬不动；取消选择之后也搬不动", async () => {
  const { state, calls } = harness();
  await state.load();
  await state.confirmMove();
  assert.equal(calls.includes("move"), false, "没选位置就不该搬");

  await state.pickDir("选择文件夹");
  assert.equal(state.picked.value?.path, "别处的稿子");
  await state.forget();
  assert.equal(state.picked.value, null);
  await state.confirmMove();
  assert.equal(calls.includes("move"), false, "取消过的选择不该还能拿来搬");
});

test("选位置时取消（壳回 null）不会把上一次的选择当成新的", async () => {
  const { state } = harness({ pick: async () => null });
  await state.load();
  await state.pickDir("选择文件夹");
  assert.equal(state.picked.value, null, "取消就是没选，不能留一个旧选择在手里");
});

test("选中一个位置之后，先前的错误提示要被清掉", async () => {
  let attempt = 0;
  const { state } = harness({
    pick: async () => {
      attempt += 1;
      if (attempt === 1) throw new Error("对话框打不开");
      return picked("另一处/稿件");
    },
  });
  await state.load();
  await state.pickDir("选择文件夹");
  assert.ok(state.error.value, "第一次失败要有提示");
  await state.pickDir("选择文件夹");
  assert.equal(state.error.value, null, "新选中的位置不该背着上一次的错误");
  assert.equal(state.picked.value?.path, "另一处/稿件");
});

test("搬完就一直是忙的（窗口马上要重启，不留一个可以再按一遍的按钮）", async () => {
  const { state } = harness();
  await state.load();
  await state.pickDir("选择文件夹");
  await state.confirmMove();
  assert.equal(state.moved.value?.files, 9);
  assert.equal(state.busy.value, true, "重启前不能让人再按一次");
});
