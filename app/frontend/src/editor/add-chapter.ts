// 点「+」之后的编排：**先问一嘴路标，再决定建不建新章**。
//
// 视图只回答"作者点了哪一行的 +"，这里回答"接下来发生什么"：
//   ① 先问这一层该不该补（删章路标，见 editor/gaps.ts）；
//   ② 有该问的就摆弹窗、记住落点，等作者拿主意；
//   ③ 他答完（补写 / 稍后 / 不用了）再照他原来点「+」的意图接着走。
//
// 为什么不在视图里写：这是**编排**不是渲染。放在视图里会让目录树多一个变化理由
// （既要画树、又要管建章流程），也没法脱离界面单测。依赖全部注入，所以这里可以单测。

import { computed, ref, type ComputedRef, type Ref } from "vue";

import type { ChapterGap } from "../api/core";
import type { Directory } from "./directory";
import type { Gaps } from "./gaps";
import { addIntent, type TreeRow } from "./tree.ts";

export interface AddChapterDeps {
  /** 删章路标那一问（editor/gaps.ts） */
  gaps: Gaps;
  /** 目录树：往里新建 / 补写后重拉 */
  directory: Pick<Directory, "create" | "refresh">;
  /** 打开"刚新建/补写"的那一章：**要接着写**（焦点策略见 editor/focus.ts） */
  openFreshChapter: (node_id: number) => Promise<void>;
  /** 会话：在某一章后面插一章（走核心既有的"插在这一章之后"）；返回新章 id，没建成是 null */
  addChapterAfter: (node_id: number) => Promise<number | null>;
}

export interface AddChapter {
  /** 弹窗该不该显示（有该问的空缺 + 记着是哪一行） */
  visible: ComputedRef<boolean>;
  /** 摆给作者的那处空缺 */
  gap: Ref<ChapterGap | null>;
  busy: Ref<boolean>;
  /** 行上的「+」：先问一嘴，没得问就直接建 */
  addHere: (row: TreeRow) => Promise<void>;
  /** 作者选了「补写」：补完直接开写 */
  fill: () => Promise<void>;
  /** 作者选了「稍后再说 / 不用了」：记下答复，再照他原来的意图接着建章 */
  answer: (answer: "deferred" | "ignored") => Promise<void>;
  /** 去回收站看看 / 点遮罩：**不记答复**，下次点「+」还会问 */
  dismiss: () => void;
}

export function useAddChapter(deps: AddChapterDeps): AddChapter {
  /** 弹出那一问的那一行：答完照他点「+」的意图接着走 */
  const pending = ref<TreeRow | null>(null);

  /**
   * 「+」连点的**续接点**：上一次是从哪一行建出来的、建成了谁。
   *
   * 为什么要它：连点同一行的「+」是最常见的动作（一口气排十章）。若每次都锚在原来那一章上，
   * 新章会一遍遍插在同一位置——屏幕上看着是**倒着长**的（第10章 → 第12章 → 第11章）。
   * 所以记住"上一次从这一行建出来的那一章"，下一次接着它往下排。
   *
   * 换一行点「+」就当没有这回事：位置仍然只管"**点哪儿插哪儿**"（那是既有规矩，有测试盯着）。
   *
   * ⚠️ 核心那边现在也管这件事：标题留空（号由核心取）时，新章**按号归位**——落在同层最后一个
   * 有编号的章之后（见 `store::node_edit`）。所以编号正常的层里这条链只是锦上添花；编号认不出来
   * 的层（序章 / 自起的名字）没有号可归位，还得靠它接着往下排，别删。
   */
  const lastAdd = ref<{ from: number; created: number } | null>(null);
  /** 答完路标那一问之后该锚在谁后面（点「+」那一刻就定下来） */
  const pendingAnchor = ref<number | null>(null);

  /** 这一行这次该锚在谁后面。 */
  function anchorFor(row: TreeRow): number {
    return lastAdd.value?.from === row.id ? lastAdd.value.created : row.id;
  }

  /** 新章落在哪一层：容器往里加，章就是它自己那一层（与下面 addByIntent 的分支一一对应）。 */
  function layerOf(row: TreeRow): number | null {
    return addIntent(row) === "inside" ? row.id : row.parent_id;
  }

  /** 作者原来点「+」的意图：容器往里加一章，章就锚在 `anchor` 后面插一章。 */
  async function addByIntent(row: TreeRow, anchor: number): Promise<void> {
    if (addIntent(row) === "inside") {
      // 标题留空＝由核心按同层序号取名
      const created = await deps.directory.create(row.id, "chapter", "");
      lastAdd.value = null; // 往里加：不算"接着往下排"的那条线
      if (created !== null) await deps.openFreshChapter(created);
      return;
    }
    // 走核心既有的"插在这一章之后"；记住建成了谁，供下一次连点接续
    const created = await deps.addChapterAfter(anchor);
    lastAdd.value = created === null ? null : { from: row.id, created };
  }

  return {
    visible: computed(() => pending.value !== null && deps.gaps.pending.value !== null),
    gap: deps.gaps.pending,
    busy: deps.gaps.busy,
    addHere: async (row) => {
      const anchor = anchorFor(row);
      if (await deps.gaps.check(layerOf(row))) {
        pending.value = row; // 有该问的空缺：先摆弹窗，别再顺手建新章
        pendingAnchor.value = anchor; // 点「+」那一刻的意图定下来，答完照它走
        return;
      }
      await addByIntent(row, anchor);
    },
    fill: async () => {
      const created = await deps.gaps.fill();
      if (created === null) return; // 没补成（错误已报过）：弹窗还摆着
      pending.value = null;
      await deps.directory.refresh(); // 补出来的章得看得见
      await deps.openFreshChapter(created); // 补写出来的空章：直接接着写
    },
    answer: async (next) => {
      const row = pending.value;
      if (!(await deps.gaps.answer(next))) return; // 答复记不下来就别偷偷往下走
      pending.value = null;
      if (row) await addByIntent(row, pendingAnchor.value ?? anchorFor(row));
      pendingAnchor.value = null;
    },
    dismiss: () => {
      pending.value = null;
      deps.gaps.dismiss();
    },
  };
}
