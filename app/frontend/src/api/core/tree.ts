// 目录树与自动编号。

import { call, COMMANDS } from "./transport";

/** 目录树的一个条目——**没有正文字段**（核心的类型上就没有，懒加载不靠自觉）。 */
export interface TreeNode {
  id: number;
  parent_id: number | null;
  /** 「卷 / 章 / 节 / 单篇 / 场景卡」——**界面不假设层级**，只按取值显示 */
  kind: string;
  /** 作者写的原文（含 `{$N}` 这类宏时就是模板）——**改名编辑的是它** */
  title: string;
  /** 显示用的那一份：宏已按同层位置渲染（`第{$N}章` → `第3章`） */
  title_rendered: string;
  word_count: number;
  /** 逐字（含标点） */
  char_count: number;
  /** 逐字（不含标点） */
  chars_no_punct: number;
  /** 已经有正文了吗（空章一眼可见） */
  has_body: boolean;
  /** 这个节点能不能编辑正文（点一下是"打开来写"还是"展开看看"） */
  holds_body: boolean;
  /** 这个节点能不能收下级（拖进来算不算数） */
  accepts_children: boolean;
  /** 下面还有没有节点（决定要不要画展开箭头） */
  has_children: boolean;
  /** 容器行的小字：本卷几章（**非容器是 0**） */
  chapter_count: number;
  section_count: number;
  /** 容器行的小字：本卷共多少字（**非容器是 0**）——按词口径 */
  subtree_word_count: number;
  /** 同上，逐字（含标点） */
  subtree_char_count: number;
  /** 同上，逐字（不含标点） */
  subtree_chars_no_punct: number;
}

/** 目录树：取某一层的子节点（**展开哪一层拉哪一层**）。 */
export const treeChildren = (work_id: number, parent_id: number | null) =>
  call<TreeNode[]>(COMMANDS.treeChildren, { work_id, parent_id });

/** 目录树：从根到该节点父级的 id 链（**一层层展开到正在写的那一章**用）。 */
export const treeAncestors = (node_id: number) => call<number[]>(COMMANDS.treeAncestors, { node_id });

/** 目录树：新建节点（卷 / 章 / …），返回新节点 id；**标题留空 = 按同层取号命名**。 */
export const treeCreateNode = (
  work_id: number,
  parent_id: number | null,
  kind: string,
  title: string,
) => call<number>(COMMANDS.treeCreateNode, { work_id, parent_id, kind, title });

/** 目录树：内联改名。 */
export const treeRenameNode = (node_id: number, title: string) =>
  call<void>(COMMANDS.treeRenameNode, { node_id, title });

/** 目录树：拖拽排序（挪到新父级的第 index 位；越界由核心夹到末尾）。 */
export const treeMoveNode = (node_id: number, parent_id: number | null, index: number) =>
  call<void>(COMMANDS.treeMoveNode, { node_id, parent_id, index });

/** 目录树：删掉一个节点（**软删除**，连同子树进回收站，能捞回来）。 */
export const treeDeleteNode = (node_id: number) =>
  call<number>(COMMANDS.treeDeleteNode, { node_id });

/** 每卷目标章数（目录里那行「本卷 12/30 章」的分母）；没设过是 null。 */
export const treeVolumeTarget = (work_id: number) =>
  call<number | null>(COMMANDS.treeVolumeTarget, { work_id });

/** 设定 / 清除每卷目标章数（null = 清掉）。 */
export const treeSetVolumeTarget = (work_id: number, chapters: number | null) =>
  call<void>(COMMANDS.treeSetVolumeTarget, { work_id, chapters });

/** 一处将要发生的编号写法改动（界面照着它摆预览）。 */
export interface NamingRewrite {
  node_id: number;
  kind: string;
  before: string;
  after: string;
}

/** 编号写法：先看会改哪几章、改成什么（只算不改）。 */
export const namingRewritePreview = (work_id: number) =>
  call<NamingRewrite[]>(COMMANDS.namingRewritePreview, { work_id });

/** 编号写法：照预览那份清单执行（作者点过确认才走到这里）。 */
export const namingRewriteApply = (work_id: number, rewrites: NamingRewrite[]) =>
  call<number>(COMMANDS.namingRewriteApply, { work_id, rewrites });
