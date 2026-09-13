// 点「+」之后的编排：**建在同层、落在点的那一行后面**，然后直接开写。
//
// 视图只回答"作者点了哪一行的 +"，这里回答"接下来发生什么"：
//   ① 容器（卷 / 部）：往里加一个（标题由核心给模板）；
//   ② 章 / 篇：插在点的那一行后面——**"点哪儿插哪儿"**。
//
// 号不在这里管：它是**位置的函数**（标题里写 `第{$N}章`，显示时按同层位置渲染，
// 见核心的 `numbering`）。所以插入、删除、拖动都不需要谁去"归位"或"改名补号"。
//
// 为什么不在视图里写：这是**编排**不是渲染。放在视图里会让目录树多一个变化理由
// （既要画树、又要管建章流程），也没法脱离界面单测。依赖全部注入，所以这里可以单测。

import { ref } from "vue";

import type { Directory } from "./directory";
import { addIntent, type TreeRow } from "./tree.ts";

export interface AddChapterDeps {
  /** 目录树：往里新建之后重拉 */
  directory: Pick<Directory, "create" | "refresh">;
  /** 打开"刚新建"的那一章：**要接着写**（焦点策略见 editor/focus.ts） */
  openFreshChapter: (node_id: number) => Promise<void>;
  /** 会话：在某一章后面插一章；返回新章 id，没建成是 null */
  addChapterAfter: (node_id: number) => Promise<number | null>;
}

export interface AddChapter {
  /** 行上的「+」 */
  addHere: (row: TreeRow) => Promise<void>;
}

export function useAddChapter(deps: AddChapterDeps): AddChapter {
  /**
   * 「+」连点的**续接点**：上一次是从哪一行建出来的、建成了谁。
   *
   * 为什么要它：连点同一行的「+」是最常见的动作（一口气排十章）。若每次都锚在原来那一章上，
   * 新章会一遍遍插在同一位置之前——屏幕上看着是**倒着长**的（第10章 → 第12章 → 第11章）。
   * 所以记住"上一次从这一行建出来的那一章"，下一次接着它往下排。
   *
   * 换一行点「+」就当没有这回事：位置仍然只管"**点哪儿插哪儿**"。
   */
  const lastAdd = ref<{ from: number; created: number } | null>(null);

  /** 这一行这次该锚在谁后面。 */
  function anchorFor(row: TreeRow): number {
    return lastAdd.value?.from === row.id ? lastAdd.value.created : row.id;
  }

  return {
    addHere: async (row) => {
      if (addIntent(row) === "inside") {
        // 容器往里加：标题留空＝由核心给模板（`第{$N}章` 之类）
        const created = await deps.directory.create(row.id, "chapter", "");
        lastAdd.value = null; // 往里加：不算"接着往下排"的那条线
        if (created !== null) await deps.openFreshChapter(created);
        return;
      }
      const created = await deps.addChapterAfter(anchorFor(row));
      lastAdd.value = created === null ? null : { from: row.id, created };
    },
  };
}
