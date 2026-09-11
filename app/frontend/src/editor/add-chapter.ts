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
  /** 目录树：往里新建 / 补写后重拉 / 切过去 */
  directory: Pick<Directory, "create" | "refresh" | "select">;
  /** 会话：在某一章后面插一章（走核心既有的"插在这一章之后"） */
  addChapterAfter: (node_id: number) => Promise<void>;
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

  /** 新章落在哪一层：容器往里加，章就是它自己那一层（与下面 addByIntent 的分支一一对应）。 */
  function layerOf(row: TreeRow): number | null {
    return addIntent(row) === "inside" ? row.id : row.parent_id;
  }

  /** 作者原来点「+」的意图：容器往里加一章，章就接着它往后插一章。 */
  async function addByIntent(row: TreeRow): Promise<void> {
    if (addIntent(row) === "inside") {
      // 标题留空＝由核心按同层序号取名
      const created = await deps.directory.create(row.id, "chapter", "");
      if (created !== null) await deps.directory.select(created);
      return;
    }
    await deps.addChapterAfter(row.id); // 走核心既有的"插在这一章之后"
  }

  return {
    visible: computed(() => pending.value !== null && deps.gaps.pending.value !== null),
    gap: deps.gaps.pending,
    busy: deps.gaps.busy,
    addHere: async (row) => {
      if (await deps.gaps.check(layerOf(row))) {
        pending.value = row; // 有该问的空缺：先摆弹窗，别再顺手建新章
        return;
      }
      await addByIntent(row);
    },
    fill: async () => {
      const created = await deps.gaps.fill();
      if (created === null) return; // 没补成（错误已报过）：弹窗还摆着
      pending.value = null;
      await deps.directory.refresh(); // 补出来的章得看得见
      await deps.directory.select(created);
    },
    answer: async (next) => {
      const row = pending.value;
      if (!(await deps.gaps.answer(next))) return; // 答复记不下来就别偷偷往下走
      pending.value = null;
      if (row) await addByIntent(row);
    },
    dismiss: () => {
      pending.value = null;
      deps.gaps.dismiss();
    },
  };
}
