// 集视图的**状态机**：当前这个集里有什么、加/拿掉/挪/定目标、一键建集。
//
// 分工（与稿件台同一条）：纯规则在 `collection.ts`；真命令由会话层注入；
// 这一层只管"当前打开的是哪个集、它现在长什么样"。
//
// 一条口径：**集没有正文**——它进来时不会去开编辑器（见会话里 `switchWork` 那一眼），
// 所以这里也不碰正文，只与引用表打交道。

import { computed, ref, type ComputedRef, type Ref } from "vue";

import type { ArticleEntry, CollectionView } from "../api/collection";
import { byTime, moveId, nudgeReady, sortedIds, type SortMode } from "./collection.ts";
import { t } from "../locales/index.ts";

/** 集视图要用的动作（会话层注入真命令，测试注入替身）。 */
export interface CollectionTransport {
  view: (work_id: number) => Promise<CollectionView>;
  add: (work_id: number, article_id: number) => Promise<CollectionView>;
  remove: (work_id: number, article_id: number) => Promise<CollectionView>;
  reorder: (work_id: number, ids: number[]) => Promise<CollectionView>;
  setTarget: (work_id: number, pieces: number | null, words: number | null) => Promise<CollectionView>;
  create: (title: string, ids: number[]) => Promise<CollectionView>;
  loose: () => Promise<ArticleEntry[]>;
}

export interface CollectionState {
  /** 当前打开的那个集（没打开集时是 null） */
  view: Ref<CollectionView | null>;
  /** 还没归集的散篇（攒集提示与"加入一篇"那张清单都用它） */
  loose: Ref<ArticleEntry[]>;
  busy: Ref<boolean>;
  note: Ref<string>;
  /** 攒集提示：没归集的散篇够数了 */
  nudge: ComputedRef<boolean>;
  /** 切到一个集：载入它的一屏（顺手刷新散篇清单） */
  enter: (work_id: number) => Promise<void>;
  /** 离开集（切到别的作品） */
  leave: () => void;
  refreshLoose: () => Promise<void>;
  add: (article_id: number) => Promise<void>;
  remove: (article_id: number) => Promise<void>;
  /** 上移 / 下移一格（`delta` 为 -1 / +1） */
  move: (article_id: number, delta: number) => Promise<void>;
  /** 集内一键排序（走法见 `SortMode`）；顺序没变就什么都不做 */
  sort: (mode: SortMode, caliber: string) => Promise<void>;
  setTarget: (pieces: number | null, words: number | null) => Promise<void>;
  /** 一键建集：把**还没归集的散篇**按时间排好一次装齐；返回新集的 id */
  createFromLoose: (title: string) => Promise<number | null>;
}

export interface CollectionOptions {
  transport: CollectionTransport;
  onError?: (message: string) => void;
}

export function useCollection(options: CollectionOptions): CollectionState {
  const view = ref<CollectionView | null>(null);
  const loose = ref<ArticleEntry[]>([]);
  const busy = ref(false);
  const note = ref("");

  const nudge = computed(() => nudgeReady(loose.value.length));

  const report = (error: unknown) => {
    options.onError?.(error instanceof Error ? error.message : String(error));
  };

  async function refreshLoose(): Promise<void> {
    try {
      loose.value = await options.transport.loose();
    } catch (error) {
      report(error);
    }
  }

  /** 把一次"会改这个集的动作"包起来：忙标记 + 落回新的一屏 + 报错，一处收口。
   *
   *  动作返回 `null` 表示"这一步没改什么"（比如上移到头了）：那就别动屏上那份。 */
  async function act(op: (work_id: number) => Promise<CollectionView | null>): Promise<void> {
    const current = view.value;
    if (busy.value || !current) return;
    busy.value = true;
    try {
      const next = await op(current.id);
      if (next) view.value = next;
      await refreshLoose();
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  const ids = () => (view.value?.entries ?? []).map((entry) => entry.id);

  return {
    view,
    loose,
    busy,
    note,
    nudge,
    enter: async (work_id) => {
      try {
        view.value = await options.transport.view(work_id);
        await refreshLoose();
      } catch (error) {
        view.value = null;
        report(error);
      }
    },
    leave: () => {
      view.value = null;
      note.value = "";
    },
    refreshLoose,
    add: (article_id) => act((work_id) => options.transport.add(work_id, article_id)),
    remove: (article_id) => act((work_id) => options.transport.remove(work_id, article_id)),
    move: (article_id, delta) =>
      act((work_id) => {
        const from = ids().indexOf(article_id);
        const next = moveId(ids(), from, from + delta);
        // 上移到头 / 下移到尾：原样返回 null，界面那份不该跟着动
        return next === ids()
          ? Promise.resolve(null)
          : options.transport.reorder(work_id, next).then((fresh) => {
              note.value = t("collection.note_moved");
              return fresh;
            });
      }),
    sort: (mode, caliber) =>
      act((work_id) => {
        const current = ids();
        const next = sortedIds(view.value?.entries ?? [], mode, caliber);
        // 已经是这个顺序就别白跑一趟（重排那次写也省了）
        const same = next.length === current.length && next.every((id, index) => id === current[index]);
        return same
          ? Promise.resolve(null)
          : options.transport.reorder(work_id, next).then((fresh) => {
              note.value = t("collection.note_sorted");
              return fresh;
            });
      }),
    setTarget: (pieces, words) => act((work_id) => options.transport.setTarget(work_id, pieces, words)),
    createFromLoose: async (title) => {
      if (busy.value) return null;
      busy.value = true;
      try {
        const picked = byTime(loose.value);
        const created = await options.transport.create(
          title.trim(),
          picked.map((entry) => entry.id),
        );
        note.value = t("collection.note_created", { title: created.title, count: created.pieces });
        await refreshLoose();
        return created.id;
      } catch (error) {
        report(error);
        return null;
      } finally {
        busy.value = false;
      }
    },
  };
}
