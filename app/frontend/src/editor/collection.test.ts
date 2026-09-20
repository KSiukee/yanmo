// 集视图纯规则的验收：**进度按什么口径比、攒集阈值、上移下移、一键建集的排序**。
//
// 这一份不碰界面也不碰核心（真命令由会话层注入），钉的是设计里的三条口径：
// 目标是参考线（没填就不显示进度）、字数按当前口径比、上移到头不动作。

import { test } from "node:test";
import assert from "node:assert/strict";

import type { ArticleEntry, CollectionView } from "../api/collection";
import {
  LOOSE_ENOUGH,
  byLength,
  byTime,
  entryWords,
  moveId,
  nudgeReady,
  pieceProgress,
  wordProgress,
} from "./collection.ts";

function view(partial: Partial<CollectionView> = {}): CollectionView {
  return {
    id: 1,
    title: "自选",
    entries: [],
    target_pieces: null,
    target_words: null,
    pieces: 0,
    words: 0,
    chars: 0,
    chars_no_punct: 0,
    ...partial,
  };
}

function entry(partial: Partial<ArticleEntry> = {}): ArticleEntry {
  return {
    id: 1,
    title: "一篇",
    status: "draft",
    word_count: 10,
    char_count: 20,
    chars_no_punct: 18,
    updated_at: 0,
    ...partial,
  };
}

test("目标是参考线：没设过就没有目标，也不会说「够了」", () => {
  const progress = pieceProgress(view({ pieces: 3 }));
  assert.equal(progress.have, 3);
  assert.equal(progress.want, null);
  assert.equal(progress.done, false, "没定目标就无所谓够没够");
});

test("篇数进度：到了才算够", () => {
  assert.equal(pieceProgress(view({ pieces: 7, target_pieces: 8 })).done, false);
  assert.equal(pieceProgress(view({ pieces: 8, target_pieces: 8 })).done, true);
  assert.equal(pieceProgress(view({ pieces: 9, target_pieces: 8 })).done, true);
});

test("字数进度按当前口径比：一个数都不许拿错格子", () => {
  const one = view({ words: 100, chars: 260, chars_no_punct: 240, target_words: 250 });
  assert.equal(wordProgress(one, "chars").have, 260);
  assert.equal(wordProgress(one, "chars_no_punct").have, 240);
  assert.equal(wordProgress(one, "words").have, 100);
  assert.equal(wordProgress(one, "chars").done, true, "260 ≥ 250");
  assert.equal(wordProgress(one, "words").done, false, "100 < 250");
});

test("攒集提示：够 N 篇才提，阈值是提示不是门槛", () => {
  assert.equal(LOOSE_ENOUGH, 5);
  assert.equal(nudgeReady(LOOSE_ENOUGH - 1), false);
  assert.equal(nudgeReady(LOOSE_ENOUGH), true);
  assert.equal(nudgeReady(0), false);
});

test("上移下移：越界与原地不动都原样返回，且不改原数组", () => {
  const ids = [1, 2, 3];
  assert.deepEqual(moveId(ids, 0, -1), [1, 2, 3], "已经在头上");
  assert.deepEqual(moveId(ids, 2, 3), [1, 2, 3], "已经在尾上");
  assert.deepEqual(moveId(ids, 1, 1), [1, 2, 3], "原地");
  assert.deepEqual(moveId(ids, 2, 1), [1, 3, 2], "往上挪一格");
  assert.deepEqual(moveId(ids, 0, 2), [2, 3, 1], "往下挪两格");
  assert.deepEqual(ids, [1, 2, 3], "原数组不许被改（界面那份还在用）");
});

test("一键建集的排序：默认按最后动过的时间，也可以按字数", () => {
  const older = entry({ id: 1, updated_at: 100, char_count: 900, chars_no_punct: 900 });
  const newer = entry({ id: 2, updated_at: 900, char_count: 100, chars_no_punct: 100 });
  assert.deepEqual(byTime([older, newer]).map((item) => item.id), [2, 1]);
  assert.deepEqual(byLength([older, newer], "chars").map((item) => item.id), [1, 2]);
});

test("篇目那一行的字数也按当前口径取", () => {
  const one = entry({ word_count: 3, char_count: 30, chars_no_punct: 27 });
  assert.equal(entryWords(one, "chars"), 30);
  assert.equal(entryWords(one, "chars_no_punct"), 27);
  assert.equal(entryWords(one, "words"), 3);
});
