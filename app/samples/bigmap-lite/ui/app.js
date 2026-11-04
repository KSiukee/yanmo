// 大地图的界面胶水：从桥里读自己的 map.json，画地点环、人物环、伏笔与章节滑块。
// 只用只读桥（window.yanmo.data），不联网、不碰宿主其它东西。
(function () {
  "use strict";
  const SVG_NS = "http://www.w3.org/2000/svg";

  const state = { doc: null, slider: 0 };
  const nodes = {
    title: document.getElementById("title"),
    stats: document.getElementById("stats"),
    svg: document.getElementById("map"),
    slider: document.getElementById("slider"),
    chapterLabel: document.getElementById("chapter-label"),
    chapterList: document.getElementById("chapters"),
    foreshadows: document.getElementById("foreshadows"),
    recompute: document.getElementById("recompute"),
    hint: document.getElementById("hint"),
  };

  function el(tag, attrs, text) {
    const node = document.createElement(tag);
    Object.keys(attrs || {}).forEach(function (key) {
      node.setAttribute(key, attrs[key]);
    });
    if (text !== undefined) node.textContent = text;
    return node;
  }

  function svgEl(tag, attrs, text) {
    const node = document.createElementNS(SVG_NS, tag);
    Object.keys(attrs || {}).forEach(function (key) {
      node.setAttribute(key, attrs[key]);
    });
    if (text !== undefined) node.textContent = text;
    return node;
  }

  function draw() {
    const doc = state.doc;
    if (!doc) return;
    const width = nodes.svg.clientWidth || 640;
    const height = nodes.svg.clientHeight || 420;
    nodes.svg.setAttribute("viewBox", "0 0 " + width + " " + height);
    nodes.svg.replaceChildren();

    const centerX = width / 2;
    const centerY = height / 2;
    const outer = Math.min(width, height) * 0.34;
    const inner = outer * 0.55;

    const visiblePlaces = BigMap.placesUpTo(doc, state.slider);
    const places = visiblePlaces.map(function (entry) { return entry.place; });
    const persons = doc.persons || [];
    const placePoints = BigMap.ringLayout(places.length, centerX, centerY, outer);
    const personPoints = BigMap.ringLayout(persons.length, centerX, centerY, inner);

    // 人物到地点的连线：没有关系数据，只画"同一本书里的归属"这层弱关联（虚线、不假装是剧情关系）
    placePoints.forEach(function (point) {
      personPoints.forEach(function (innerPoint) {
        nodes.svg.appendChild(
          svgEl("line", {
            x1: innerPoint.x,
            y1: innerPoint.y,
            x2: point.x,
            y2: point.y,
            class: "link",
          }),
        );
      });
    });

    placePoints.forEach(function (point, index) {
      const place = places[index];
      const unknown = !visiblePlaces[index].known;
      nodes.svg.appendChild(
        svgEl("circle", {
          cx: point.x,
          cy: point.y,
          r: 26,
          class: unknown ? "place unknown-when" : "place",
        }),
      );
      nodes.svg.appendChild(svgEl("text", { x: point.x, y: point.y + 4, class: "label" }, place.name));
      if (unknown) {
        nodes.svg.appendChild(
          svgEl("text", { x: point.x, y: point.y + 44, class: "label small" }, "何时出现不明"),
        );
      }
    });
    personPoints.forEach(function (point, index) {
      const person = persons[index];
      nodes.svg.appendChild(svgEl("circle", { cx: point.x, cy: point.y, r: 18, class: "person" }));
      nodes.svg.appendChild(svgEl("text", { x: point.x, y: point.y + 4, class: "label small" }, person.name));
    });

    nodes.stats.textContent =
      "到这一章：地点 " + places.length + " · 人物 " + persons.length +
      " · 未收线头 " + BigMap.foreshadowsUpTo(doc, state.slider).length +
      "（全书 " + doc.chapters.length + " 章 / " + (doc.places || []).length + " 个地点）";
    nodes.title.textContent = doc.work_title || "（未命名）";
    renderChapters();
    renderForeshadows();
  }

  function renderChapters() {
    const doc = state.doc;
    nodes.chapterList.replaceChildren();
    (doc.chapters || []).forEach(function (chapter, index) {
      const active = index === state.slider;
      const row = el("li", { class: active ? "chapter active" : "chapter" }, chapter.title || "（无标题）");
      if (chapter.lines && chapter.lines.length > 0) {
        row.appendChild(el("span", { class: "lines" }, chapter.lines.join(" / ")));
      }
      nodes.chapterList.appendChild(row);
    });
    const current = (doc.chapters || [])[state.slider];
    nodes.chapterLabel.textContent = current
      ? "第 " + (state.slider + 1) + " 章：" + (current.title || "（无标题）")
      : "这本书还没有章节";
  }

  function renderForeshadows() {
    const doc = state.doc;
    const lines = BigMap.foreshadowsUpTo(doc, state.slider);
    nodes.foreshadows.replaceChildren();
    if (lines.length === 0) {
      nodes.foreshadows.appendChild(el("li", { class: "empty" }, "到这一章为止，没有还没收的线头。"));
      return;
    }
    lines.forEach(function (line) {
      const index = BigMap.chapterOf(doc, line.planted_node);
      const where = index >= 0 ? "第 " + (index + 1) + " 章埋下" : "埋在哪一章没记";
      const item = el("li", {}, line.gist || "（无内容）");
      item.appendChild(el("span", { class: "where" }, where));
      if (line.note) item.appendChild(el("span", { class: "note" }, line.note));
      nodes.foreshadows.appendChild(item);
    });
  }

  function wireSlider() {
    nodes.slider.addEventListener("input", function () {
      state.slider = Number(nodes.slider.value);
      renderChapters();
      renderForeshadows();
      draw();
    });
  }

  /** 让模块自己重算一次（调 run），跑完再把自己的数据读回来。 */
  async function recompute() {
    const button = nodes.recompute;
    if (!button || button.disabled) return;
    button.disabled = true;
    button.textContent = "重算中…";
    try {
      await window.yanmo.run();
      for (let attempt = 0; attempt < 60; attempt += 1) {
        await new Promise(function (resolve) { setTimeout(resolve, 500); });
        const status = await window.yanmo.status();
        if (!status.running) break;
        if (status.error) throw new Error(status.error);
      }
      await loadAndDraw();
    } catch (error) {
      nodes.hint.textContent = "重算失败了：" + (error.code || error.message || error);
    } finally {
      button.disabled = false;
      button.textContent = "重算";
    }
  }

  async function loadAndDraw() {
    const result = await window.yanmo.data("map.json");
    if (!result || result.value === null) {
      nodes.hint.textContent = "还没有地图数据：点右上角「重算」生成一份。";
      return false;
    }
    state.doc = JSON.parse(result.value);
    state.slider = Math.max(0, (state.doc.chapters || []).length - 1);
    nodes.slider.max = String(Math.max(0, (state.doc.chapters || []).length - 1));
    nodes.slider.value = String(state.slider);
    nodes.hint.textContent = "";
    draw();
    return true;
  }

  async function boot() {
    if (!window.yanmo) {
      nodes.hint.textContent = "桥没挂上：这个页面没有拿到宿主客户端。";
      return;
    }
    try {
      wireSlider();
      if (nodes.recompute) nodes.recompute.addEventListener("click", recompute);
      await loadAndDraw();
    } catch (error) {
      nodes.hint.textContent = "读自己的数据失败了：" + (error.code || error.message || error);
    }
  }

  window.addEventListener("resize", function () {
    if (state.doc) draw();
  });
  boot();
})();
