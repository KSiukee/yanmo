// 「出场人物」那一格：**点开选人**（一张多选卡），选完立刻落库。
//
// 为什么单独成件：它是表里唯一**不是打字改**的一格——打开时要读这本书的人物卡、
// 换书时要清干净、点一个人要走一次"整份覆盖"的写。这一族状态与命令跟"表怎么摆"
// 没有关系（那是 `grid-panel.ts`），混在一起会让那份文件既管排版又管选人。
//
// 三条分寸：
// 1. **只摆人物卡**：这一格问的是"这一章有谁出场"，设定（地点 / 物品）不往里混
//    （它们归「资料」那一屏；将来真要按场景挂地点，那是另一格的事）；
// 2. **点一下就落**：不等"确定"——一个多选框的语义就是"点了就改了"，
//    再要一次确认等于让作者按两下才等于一下；
// 3. **回执取库里的那一份**：名字是核心读回来的（卡改名之后这里显示的就是新名）。

import { ref, type Ref } from "vue";

import { entityList, type EntityCard } from "../api/entity.ts";
import { outlineSetCast, type CastMember, type OutlineRowDto } from "../api/outline.ts";

export interface GridCastOptions {
  /** 当前作品（换书＝换一份人物卡） */
  workId: Ref<number | null>;
  /** 换掉手上那一行（回执从核心读回来） */
  replace: (row: OutlineRowDto) => void;
  /** 手上那一行 */
  rowOf: (node_id: number) => OutlineRowDto | undefined;
  /** 失败时把话交给表格统一显示 */
  report: (error: unknown) => void;
  busy: Ref<boolean>;
}

export function useGridCast(deps: GridCastOptions) {
  /** 现在打开选人卡的是哪一行（同时只可能有一行） */
  const openFor = ref<number | null>(null);
  /** 这本书的人物卡（打开时读一次） */
  const cards = ref<EntityCard[]>([]);

  async function open(node_id: number) {
    openFor.value = node_id;
    const work = deps.workId.value;
    if (!work) {
      cards.value = [];
      return;
    }
    try {
      const board = await entityList(work);
      cards.value = board.cards.filter((card) => card.kind === "person");
    } catch (error) {
      deps.report(error);
    }
  }

  function close() {
    openFor.value = null;
  }

  /** 点一个人：加进去 / 拿掉，然后**整份覆盖**地存一次。 */
  async function toggle(node_id: number, entity_id: number) {
    const row = deps.rowOf(node_id);
    if (!row) return;
    const ids = row.cast.map((member) => member.entity_id);
    const next = ids.includes(entity_id)
      ? ids.filter((id) => id !== entity_id)
      : [...ids, entity_id];
    try {
      deps.busy.value = true;
      const saved: CastMember[] = await outlineSetCast(node_id, next);
      deps.replace({ ...row, cast: saved });
    } catch (error) {
      deps.report(error);
    } finally {
      deps.busy.value = false;
    }
  }

  /** 换书 / 收起表：选人卡跟着收（它指的是某一行，那一行已经不在了）。 */
  function reset() {
    openFor.value = null;
    cards.value = [];
  }

  return { openFor, cards, open, close, toggle, reset };
}
