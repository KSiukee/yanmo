// 拖动一行：**落在哪儿**翻译成一次「挪到新父级的第几位」——目录树与大纲表共用这一份。
//
// 为什么单列一个纯文件、两处共用：这一段有一个**极容易差一格**的地方。
// 核心的 `move_node` 是"先把这一行摘出去，再插到第 index 位"（同级重排成密集序号，
// 见 `Store::move_node` 的 `renumber`），所以**往下拖**时必须把它自己占的那一格减掉。
//
// 目录树原来那版没减：把第 1 章拖到第 2 章后面，算出来的 index 让第 1 章落到第 3 章后面
// （往下拖一格 = 掉两格）；把第 1 章拖到第 2 章前面，反而变成换位。这类错不报任何错，
// 只是"手感奇怪"，只有把判据收在一处、用例子钉住才不会双份地犯。

/** 拖动要用的那点行信息（两边的行对象各有各的字段，取这两个就够）。 */
export interface DropRow {
  id: number;
  parent_id: number | null;
}

/** 落点：排在目标前面 / 放进目标里 / 排在目标后面。 */
export type DropZone = "before" | "inside" | "after";

export interface DropPlan {
  parent_id: number | null;
  /** 新父级下的第几位（`inside` 给"追加到末尾"，越界由核心夹） */
  index: number;
}

/** 追加到末尾（核心会把越界的下标夹到末尾）。 */
const APPEND = Number.MAX_SAFE_INTEGER;

/**
 * `node` 是不是在 `ancestor` 的子树里（含它自己）。
 *
 * 走父链而不是递归：链长就是树的深度，而数据真坏（成环）时也没关系——
 * 走到 `rows.length` 步就停，绝不转不完。
 */
export function inSubtree(rows: DropRow[], node: number, ancestor: number): boolean {
  let current: number | null = node;
  for (let step = 0; step <= rows.length && current !== null; step += 1) {
    if (current === ancestor) return true;
    current = rows.find((row) => row.id === current)?.parent_id ?? null;
  }
  return false;
}

/**
 * 落点 → `(新父级, 第几位)`；放不下给 `null`（拖进自己或自己的子孙里）。
 *
 * - `inside`：只有当目标是容器（卷 / 分组行）才由调用方允许；这里只管"放进去、排末尾"；
 * - `before` / `after`：与目标**同一个父级**，插在它前 / 后——下标按"摘掉自己之后"算。
 */
export function dropPlan(
  rows: DropRow[],
  draggedId: number,
  targetId: number,
  zone: DropZone,
): DropPlan | null {
  if (draggedId === targetId) return null;
  const target = rows.find((row) => row.id === targetId);
  // 拖的那一行或落点那一行不在表里（被别处删了 / 已经不是这一屏了）：不给落点
  if (!target || !rows.some((row) => row.id === draggedId)) return null;

  const parent_id = zone === "inside" ? target.id : target.parent_id;
  // 拖进自己那一支里 = 成环，核心也会拒——这里先说清，不画一个骗人的落点
  if (parent_id !== null && inSubtree(rows, parent_id, draggedId)) return null;
  if (zone === "inside") return { parent_id, index: APPEND };

  const siblings = rows.filter((row) => row.parent_id === target.parent_id);
  const targetAt = siblings.findIndex((row) => row.id === targetId);
  if (targetAt < 0) return null;
  let index = targetAt + (zone === "after" ? 1 : 0);
  const draggedAt = siblings.findIndex((row) => row.id === draggedId);
  // 同一个父级里、自己原来排在目标前面：摘掉自己之后，目标的位置要往前挪一格
  if (draggedAt >= 0 && draggedAt < index) index -= 1;
  return { parent_id, index: Math.max(0, index) };
}
