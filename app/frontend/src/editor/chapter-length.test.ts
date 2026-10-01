// 章长提醒的验收：**只在停下点读、判据来自核心（界面只念事实）、答复两态**。
//
// 全部打的是纯逻辑那一半（`editor/chapter-length.ts`）：通道是假的，所以"读了没有、读了什么、
// 说了什么"都看得见。判据本身（中位数 / 样本门槛 / 倍数）在 Rust 侧
// （`crates/yanmo-core/tests/chapter_length.rs`）——**界面不重写一遍**。

import { test } from "node:test";
import assert from "node:assert/strict";

import type { ChapterLengthBoard } from "../api/chapter-length.ts";
import { has, t } from "../locales/index.ts";
import {
  chapterLengthBoard,
  chapterTipText,
  isLongChapter,
  markKept,
  pickChapterNudge,
  useChapterLengthWatch,
  type ChapterLengthTransport,
} from "./chapter-length.ts";

/** 一页事实的一行：`[节点 id, 字符数, 偏不偏长, 说过就这样写没有]`。 */
type Row = [number, number, boolean, boolean?];

/** 核心那一页的形状（判据已经算好；这里只摆结果）。 */
function board(rows: Row[], median = 2000): ChapterLengthBoard {
  return {
    median,
    ratio: 2,
    min_samples: 3,
    judged: median > 0,
    chapters: rows.map(([node_id, chars, long, dismissed]) => ({
      node_id,
      chars,
      long,
      dismissed: dismissed ?? false,
      fingerprint: `chapter-long:${node_id}`,
    })),
  };
}

/** 假通道：读的页码由测试摆（`null` = 读不出来），写记在自己账上。 */
function fake(page: ChapterLengthBoard | null, options: { keepFails?: boolean } = {}) {
  const readWorks: number[] = [];
  const kept: string[] = [];
  const transport: ChapterLengthTransport = {
    report: async (work_id: number) => {
      readWorks.push(work_id);
      if (page === null) throw new Error("读不出来");
      return page;
    },
    keep: async (_work_id: number, fingerprint: string) => {
      if (options.keepFails) throw new Error("写不进去");
      kept.push(fingerprint);
    },
  };
  return { transport, readWorks, kept };
}

test("样本不足 / 不偏长 / 已经说过「就这样写」：都不念", () => {
  // 样本不足：核心会这样给（不判 ⇒ judged=false、每章 long=false、median=0）
  const thin = board([[1, 9000, false], [2, 1000, false]], 0);
  assert.equal(isLongChapter(thin, 1), false, "样本不足：不偏长");
  assert.equal(pickChapterNudge(thin, 7, 1), null, "样本不足不提示（哪怕这一章看着挺长）");

  // 正常长度：都在中位数附近
  const normal = board([[1, 2000, false], [2, 2200, false]]);
  assert.equal(pickChapterNudge(normal, 7, 2), null, "正常长度的章不提示");

  // 偏长，但作者说过「就这样写」
  const kept = board([[1, 4700, true, true]]);
  assert.equal(isLongChapter(kept, 1), false, "「就这样写」之后：不再提，目录里也不标");
});

test("念的是事实：这一章多少字、这本书中位多少、几倍", () => {
  const tip = pickChapterNudge(board([[1, 4700, true]]), 7, 1);
  assert.deepEqual(
    tip,
    {
      work_id: 7,
      node_id: 1,
      chars: 4700,
      median: 2000,
      times: 2.4,
      fingerprint: "chapter-long:1",
    },
    "中位数与真实字数原样念出来（倍数一位小数）",
  );
  assert.equal(
    chapterTipText(tip!),
    "这一章 4700 字，这本书中位 2000 字（2.4 倍）——是故意的吗？",
    "末尾是问句，不是命令",
  );
  assert.ok(has("break.chapter_keep"), "「就这样写」在字典里");
  assert.equal(t("break.chapter_keep"), "就这样写");
  assert.equal(t("break.dismiss"), "知道了", "另一个答复照旧是「知道了」（只关这一次）");
});

test("「就这样写」之后：同一章不再念，别的章照旧", () => {
  const before = board([[1, 4700, true], [2, 9000, true]]);
  assert.ok(pickChapterNudge(before, 7, 1), "先还念着");

  const after = markKept(before, 1)!;
  assert.equal(pickChapterNudge(after, 7, 1), null, "记上之后这一章不再念");
  assert.ok(pickChapterNudge(after, 7, 2), "别的章照旧");
  assert.equal(before.chapters[0].dismissed, false, "改的是本地那份副本，不是把读回来那份也改了");
});

test("★停下点之前一次都不读：击键那条路上一句都不评估", async () => {
  const env = fake(board([[1, 4700, true]]));
  const watch = useChapterLengthWatch(env.transport);
  // 模块级那份是共享的（别的用例可能已经摆过一份）：这里只比"这一次有没有读、有没有变"
  const before = chapterLengthBoard().value;

  assert.deepEqual(env.readWorks, [], "还没到停下点：一次都没读过");
  assert.equal(chapterLengthBoard().value, before, "也没有摆出任何新事实");

  assert.ok(await watch.atStop(7, 1), "到了停下点才读一眼");
  assert.deepEqual(env.readWorks, [7], "读的就是这本书");
});

test("读不出来一声不响（不拿猜的数字开口）", async () => {
  const env = fake(null);
  const watch = useChapterLengthWatch(env.transport);
  const before = chapterLengthBoard().value;
  assert.equal(await watch.atStop(7, 1), null, "读不出来：一句话都不说");
  assert.equal(chapterLengthBoard().value, before, "读失败也不把手里的那份改坏");
});

test("「知道了」只关这一次：下一个停下点还超长就再提一次", async () => {
  const env = fake(board([[1, 4700, true]]));
  const watch = useChapterLengthWatch(env.transport);
  assert.ok(await watch.atStop(7, 1), "第一次停下点说了");
  // 界面那侧的「知道了」只是把条收掉（不记账）——等价于同一个停下点再问一次
  assert.ok(await watch.atStop(7, 1), "没记账 ⇒ 下次停下来还说得出来");
});

test("「就这样写」：记号照抄核心给的那一条，本地这份也记上", async () => {
  const env = fake(board([[1, 4700, true]]));
  const watch = useChapterLengthWatch(env.transport);
  await watch.atStop(7, 1);
  await watch.keep(7, 1, "chapter-long:1");

  assert.deepEqual(env.kept, ["chapter-long:1"], "写的是核心给的记号原文（界面不自己拼 identity）");
  assert.equal(isLongChapter(chapterLengthBoard().value, 1), false, "本地也记上了：角标立刻消失");
});

test("写不进去就不假装记上（下回读库还是「没说过」）", async () => {
  const env = fake(board([[1, 4700, true]]), { keepFails: true });
  const watch = useChapterLengthWatch(env.transport);
  await watch.atStop(7, 1);
  await watch.keep(7, 1, "chapter-long:1");

  assert.deepEqual(env.kept, [], "没写进去");
  assert.equal(isLongChapter(chapterLengthBoard().value, 1), true, "就当没说过：这一章以后还会再提");
});

test("换书只把分布读进来（不发声）", async () => {
  const env = fake(board([[1, 4700, true]]));
  const watch = useChapterLengthWatch(env.transport);
  await watch.refresh(7);
  assert.deepEqual(env.readWorks, [7], "读了一眼");
  assert.ok(chapterLengthBoard().value, "事实摆上来（目录角标要它）");
});

test("★两次停下点挨得近：只认最新那一次（不拿上一章的数字开口）", async () => {
  const reads: Array<(page: ChapterLengthBoard) => void> = [];
  const transport: ChapterLengthTransport = {
    // 手动放行的读：用来摆"先发的还没回来，后发的先回来了"
    report: (work_id: number) =>
      new Promise<ChapterLengthBoard>((resolve) => {
        void work_id;
        reads.push(resolve);
      }),
    keep: async () => undefined,
  };
  const watch = useChapterLengthWatch(transport);

  const first = watch.atStop(7, 1); // 切章：刚离开第 1 章
  const second = watch.atStop(7, 2); // 紧接着关窗：刚离开第 2 章
  reads[1]!(board([[1, 1000, false], [2, 4700, true]]));
  const latest = await second;
  assert.equal(latest?.node_id, 2, "后发的那次：说的是第 2 章");

  reads[0]!(board([[1, 4700, true], [2, 1000, false]]));
  assert.equal(await first, null, "先发那次来晚了：作废（它已经不是「刚离开的那一章」）");
});
