// 目录树状态：**一层一问**的树 + 展开 / 收起 / 结构编辑。
//
// 三条纪律：
// 1. **懒加载**：只有展开过的层级才有数据；打开作品只拉根那一层，长篇的几百章不会一次全进来；
// 2. 拿了正文就渲染不动**结构**——结构编辑（新建 / 改名 / 拖动）后**只重拉受影响的那一层**，
//    不整树刷新（改一处抖全身会闪、还会把展开状态冲掉）；
// 3. 界面只读这里算好的**扁平行**（含缩进与展开态），组件不自己拼树。
//
// 这里只认接口不认具体命令（真命令在会话层注入）：树的状态机可以脱离界面与核心单测。

import type { TreeNode } from "../api/core";
import { formatWords } from "./display.ts";
import { t } from "../locales/index.ts";
import type { Counts } from "./wordcount.ts";
import { pickCount } from "./wordcount.ts";

/**
 * 交给界面渲染的一行——扁平化 + 缩进 + 展开态。
 *
 * **这里不是又一份字段清单**：它直接继承核心那份 `TreeNode`，只多加两个"界面态"字段。
 * 曾经这里手抄过一份字段表，抄漏了 `char_count` / `chars_no_punct` / `subtree_char_count`——
 * 运行时对象是整份展开的（模板读得到），类型上却"没有这个字段"，
 * 于是"换了口径字数就不刷新"这类毛病在类型层面也照不出来（纯 tsc 不查模板）。
 */
export interface TreeRow extends TreeNode {
  depth: number;
  expanded: boolean;
}

/** 树要用的几个动作（会话层注入真命令，测试注入替身）。 */
export interface TreeTransport {
  children: (work_id: number, parent_id: number | null) => Promise<TreeNode[]>;
  /** 从根到该节点父级的 id 链（"展开到这一章"用） */
  ancestors: (node_id: number) => Promise<number[]>;
  create: (work_id: number, parent_id: number | null, kind: string, title: string) => Promise<number>;
  rename: (node_id: number, title: string) => Promise<void>;
  /** 软删除（进回收站）：核心会连带整棵子树 */
  remove: (node_id: number) => Promise<number>;
  move: (node_id: number, parent_id: number | null, index: number) => Promise<void>;
}

/** 根层的键：节点 id 从 1 开始，0 不会撞上。 */
const ROOT = 0;

/**
 * 行上的「+」该干什么：
 * - 能写正文的 → `after`：**接着它往后插一章**（写作时最常用的那个动作）；
 * - 只能装东西的容器 → `inside`：往这一层里加一章；
 * - 其余（叶子又收不了下级）→ `null`：不该画这个按钮。
 *
 * 先问"能不能写正文"再问"能不能装"——章两样都占，但作者的意图是"再来一章"。
 */
export function addIntent(row: Pick<TreeRow, "holds_body" | "accepts_children">): "after" | "inside" | null {
  if (row.holds_body) return "after";
  if (row.accepts_children) return "inside";
  return null;
}

/**
 * 容器行右侧那行小字：**本卷几章**（作者设了卷长就是 `12/30章`）· 共多少字。
 *
 * 只用在校不了正文的容器行上；能写正文的行显示的是它自己那一章的字数。
 */
export function containerLabel(
  row: Pick<
    TreeRow,
    "chapter_count" | "subtree_word_count" | "subtree_char_count" | "subtree_chars_no_punct"
  >,
  target: number | null,
  /** 合计算哪个口径：**跟同一棵树上的章行用同一个口径**（不然 206 字旁边写着 200 词） */
  caliber: string,
): string {
  const total = pickCount(
    {
      word_count: row.subtree_word_count,
      char_count: row.subtree_char_count,
      chars_no_punct: row.subtree_chars_no_punct,
    },
    caliber,
  );
  if (row.chapter_count === 0 && total === 0) return t("tree.container_empty");
  const chapters =
    target && target > 0
      ? t("tree.chapter_ratio", { done: row.chapter_count, target })
      : t("tree.chapter_count", { count: row.chapter_count });
  return t("tree.container_label", { chapters, words: formatWords(total) });
}

/** 环检测的上行上限——树坏了要明确报错，不是转到天荒地老。 */
const MAX_DEPTH = 512;

export class DirectoryTree {
  private readonly transport: TreeTransport;
  private work_id: number | null = null;
  /** 这一层有哪些节点（键 = 父节点 id，根层是 0），**顺序即显示顺序** */
  private childrenOf = new Map<number, number[]>();
  /** 已经拿到的节点（懒加载下只有展开过的部分） */
  private nodes = new Map<number, TreeNode>();
  private expanded = new Set<number>();

  constructor(transport: TreeTransport) {
    this.transport = transport;
  }

  get opened(): boolean {
    return this.work_id !== null;
  }

  node(node_id: number): TreeNode | null {
    return this.nodes.get(node_id) ?? null;
  }

  /** 打开一部作品：清空旧树，只拉根那一层。 */
  async openWork(work_id: number): Promise<void> {
    this.work_id = work_id;
    this.nodes.clear();
    this.childrenOf.clear();
    this.expanded.clear();
    await this.loadLevel(null);
  }

  /** 展开 / 收起一层；展开时**当场只拉这一层**。 */
  async toggle(node_id: number): Promise<void> {
    if (this.expanded.delete(node_id)) return;
    this.expanded.add(node_id);
    await this.loadLevel(node_id);
  }

  /**
   * 把"正在写的那一章"露出来：沿着它的祖先链一层层展开。
   *
   * 章在收起的卷里时，目录上看不到高亮——打开作品、切到别的卷的章，都要把这条路摊开。
   * 已经拉过的层不重拉（切章很频繁，别为同一条路反复问核心）。
   */
  async reveal(node_id: number): Promise<void> {
    if (this.work_id === null) return;
    if (!this.childrenOf.has(ROOT)) await this.loadLevel(null);
    for (const ancestor of await this.transport.ancestors(node_id)) {
      this.expanded.add(ancestor);
      if (!this.childrenOf.has(ancestor)) await this.loadLevel(ancestor);
    }
  }

  /** 新建（卷 / 章 / …），返回新节点 id。 */
  async create(parent_id: number | null, kind: string, title: string): Promise<number> {
    const work_id = this.requireWork();
    const created = await this.transport.create(work_id, parent_id, kind, title);
    if (parent_id !== null) this.expanded.add(parent_id); // 新建完就看得见，别让人再点一次
    // 多了一个节点，各层祖先的"本卷几章 / 共多少字"都变了：重拉看得见的那些层
    await this.reloadVisible();
    return created;
  }

  /** 改名：就地改，不重拉整层（改名只动一个节点）。 */
  async rename(node_id: number, title: string): Promise<void> {
    const next = title.trim();
    const node = this.nodes.get(node_id);
    if (!next || !node || node.title === next) return; // 空标题 = 放弃；没变 = 不白跑一趟
    await this.transport.rename(node_id, next);
    node.title = next;
  }

  /** 拖动：挪到 `new_parent` 的第 `index` 位（同层拖动时索引要扣掉自己占的那一位）。 */
  async move(node_id: number, new_parent: number | null, index: number): Promise<void> {
    const node = this.nodes.get(node_id);
    if (!node || node.id === new_parent) return;
    if (new_parent !== null && this.isDescendant(node_id, new_parent)) return; // 拖进自己怀里：不干

    let target = index;
    if (node.parent_id === new_parent) {
      const siblings = this.childrenOf.get(new_parent ?? ROOT) ?? [];
      const current = siblings.indexOf(node_id);
      if (current >= 0 && target > current) target -= 1;
      if (current === target) return; // 位置没变，不跑这一趟
    }

    await this.transport.move(node_id, new_parent, target);
    if (new_parent !== null) this.expanded.add(new_parent); // 拖进去就展开，否则"东西不见了"
    // 搬动会同时改两边各层祖先的"本卷几章 / 共多少字"：整片重拉最省心（拖动不是高频动作）
    await this.reloadVisible();
  }

  /** 删掉一个节点（软删，进回收站）：删完把看得见的层重拉一遍——它的子树一起走了。 */
  async remove(node_id: number): Promise<number> {
    const removed = await this.transport.remove(node_id);
    this.expanded.delete(node_id); // 这一层不用再记着展开了
    await this.reloadVisible();
    return removed;
  }

  /** `node_id` 是不是 `ancestor_id` 自己或它的子孙（判断"删的是不是我正在写的那一支"）。 */
  contains(ancestor_id: number, node_id: number): boolean {
    return node_id === ancestor_id || this.isDescendant(ancestor_id, node_id);
  }

  /**
   * 落盘后顺手更新这一行的字数——**不为了几个字重拉一次目录**。
   *
   * **三个口径一起更新**：树上显示哪个口径由作者选的那一档决定（点一下就换一个数），
   * 只更新一个的话，选别档的那份就永远停在"上次重拉目录"的数字上——
   * 真机上看到的现象正是"章字数不跟着刷新"（状态栏 206 字，树上还写着 111）。
   * 差额同时加到各层祖先的"本卷共多少字"上，卷那一行的小字才不会越写越不准。
   */
  applyCounts(node_id: number, counts: Counts, has_body: boolean): void {
    const node = this.nodes.get(node_id);
    if (!node) return; // 这一行还没加载过：界面上也没显示它，下次重拉自然就对了
    const delta_char = counts.char_count - node.char_count;
    const delta_punct = counts.chars_no_punct - node.chars_no_punct;
    const delta_word = counts.word_count - node.word_count;
    node.char_count = counts.char_count;
    node.chars_no_punct = counts.chars_no_punct;
    node.word_count = counts.word_count;
    node.has_body = has_body;
    if (delta_char === 0 && delta_punct === 0 && delta_word === 0) return;
    let parent = node.parent_id;
    for (let guard = 0; parent !== null && guard < MAX_DEPTH; guard += 1) {
      const ancestor = this.nodes.get(parent);
      if (!ancestor) break;
      ancestor.subtree_char_count += delta_char;
      ancestor.subtree_chars_no_punct += delta_punct;
      ancestor.subtree_word_count += delta_word;
      parent = ancestor.parent_id;
    }
  }

  /**
   * 重拉"当前看得见的那些层"（根 + 已展开的每一层）。
   *
   * 用在**别处改了结构**之后（例如新建章节走的是编辑会话那条路）：只重拉看得见的层，
   * 比整树重建便宜，也不会把展开状态冲掉。
   */
  async reloadVisible(): Promise<void> {
    const levels: (number | null)[] = [null, ...this.expanded];
    for (const level of levels) await this.loadLevel(level);
  }

  /**
   * `candidate` 是不是 `ancestor` 的子孙（拖拽时拒绝把节点拖进自己的子树）。
   *
   * 只认已加载的层级——但**看得见的落点，其祖先必然都加载过**（要看得见就得一层层展开），
   * 所以这条判断在实际操作里够用；真出了格，核心那边还有一道成环检查拦着。
   */
  isDescendant(ancestor: number, candidate: number): boolean {
    let current = this.nodes.get(candidate)?.parent_id ?? null;
    for (let guard = 0; current !== null && guard < MAX_DEPTH; guard += 1) {
      if (current === ancestor) return true;
      current = this.nodes.get(current)?.parent_id ?? null;
    }
    return false;
  }

  /** 扁平化成可见行——**只含展开过的层级**，界面照着顺序画即可。 */
  rows(): TreeRow[] {
    const out: TreeRow[] = [];
    const walk = (parent: number, depth: number) => {
      for (const id of this.childrenOf.get(parent) ?? []) {
        const node = this.nodes.get(id);
        if (!node) continue;
        const expanded = this.expanded.has(id);
        out.push({ ...node, depth, expanded });
        if (expanded) walk(id, depth + 1);
      }
    };
    walk(ROOT, 0);
    return out;
  }

  private requireWork(): number {
    if (this.work_id === null) throw new Error(t("tree.no_work_open"));
    return this.work_id;
  }

  /** 重拉一层：**整层替换**，顺序以库里为准（密集序号）。 */
  private async loadLevel(parent_id: number | null): Promise<void> {
    const work_id = this.requireWork();
    const list = await this.transport.children(work_id, parent_id);
    this.childrenOf.set(parent_id ?? ROOT, list.map((node) => node.id));
    for (const node of list) this.nodes.set(node.id, node);
    // 父节点自己那一行是**上一次拉它那一层时**留下的副本，得顺手把"有没有下级"对齐：
    // 否则刚建进去的章看不见展开箭头（下一次展开才发现）——而空了的容器又还挂着箭头。
    const parent = parent_id === null ? null : this.nodes.get(parent_id);
    if (parent) parent.has_children = list.length > 0;
  }
}
