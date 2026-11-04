// 大地图的纯逻辑（无 DOM）：布局、按章节切片、统计。
// 用老式脚本定义到 globalThis 上，浏览器与测试都能直接跑（测试用 vm 加载）。
(function (root) {
  "use strict";

  // 环形布局：地点排外圈、人物排内圈。确定性（不随机），同样的数据每次画在同一处。
  function ringLayout(count, centerX, centerY, radius) {
    const points = [];
    if (count <= 0) return points;
    for (let index = 0; index < count; index += 1) {
      const angle = (Math.PI * 2 * index) / count - Math.PI / 2;
      points.push({
        x: centerX + Math.cos(angle) * radius,
        y: centerY + Math.sin(angle) * radius,
      });
    }
    return points;
  }

  // 章节节点 id → 它在目录里的序号（0 起）。找不到返回 -1。
  function chapterIndexOf(doc, nodeId) {
    if (nodeId === null || nodeId === undefined) return -1;
    const chapters = doc.chapters || [];
    for (let index = 0; index < chapters.length; index += 1) {
      if (chapters[index].id === nodeId) return index;
    }
    return -1;
  }

  // 拖到第 slider 章时，这条伏笔"已经埋下、还没收"（埋在这一章或更早）。
  function foreshadowsUpTo(doc, slider) {
    const lines = doc.open_foreshadows || [];
    return lines.filter(function (line) {
      const index = chapterIndexOf(doc, line.planted_node);
      if (index < 0) return true; // 没记埋在哪一章：一直显示，但标明"位置不明"
      return index <= slider;
    });
  }

  // 拖到第 slider 章时，这个地点"已经露出"（章节标题里出现过）。没有时间线的地点一直显示，
  // 但调用方要据此标"位置不明"——**不把"不知道"画成"不存在"**。
  function placesUpTo(doc, slider) {
    const places = doc.places || [];
    const visible = [];
    places.forEach(function (place) {
      const seen = (place.seen_in_titles || []).filter(function (index) {
        return typeof index === "number" && index >= 0;
      });
      if (seen.length === 0) {
        visible.push({ place: place, known: false });
        return;
      }
      const first = Math.min.apply(null, seen);
      if (first <= slider) visible.push({ place: place, known: true });
    });
    return visible;
  }

  // 这条伏笔在第几章埋的（给界面写"第 N 章"用）；0 起。
  function chapterOf(doc, nodeId) {
    return chapterIndexOf(doc, nodeId);
  }

  function stats(doc) {
    return {
      chapters: (doc.chapters || []).length,
      places: (doc.places || []).length,
      persons: (doc.persons || []).length,
      open: (doc.open_foreshadows || []).length,
    };
  }

  root.BigMap = {
    ringLayout: ringLayout,
    chapterIndexOf: chapterIndexOf,
    chapterOf: chapterOf,
    foreshadowsUpTo: foreshadowsUpTo,
    placesUpTo: placesUpTo,
    stats: stats,
  };
})(typeof window !== "undefined" ? window : globalThis);
