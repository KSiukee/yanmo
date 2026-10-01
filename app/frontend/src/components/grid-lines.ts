// 「五线」那一格：**点开勾选，勾完一次落库**。
//
// 为什么单独成件（与 `grid-cast.ts` 同一个理由）：它是表里第二格**不是打字改**的格子，
// 自带一份"手上正勾着的那份清单"，而且读的时候要把另一条命令（`node_lines`）的结果
// 并到表那一行上——混进 `grid-panel.ts` 会让那份文件既管排版又管这一格，而它已经贴着
// 上帝级那条线（≥400 行）。
//
// 六条分寸：
// 1. **一次编辑只发一次写**：勾一下只改手上那份（`draft`），**不发请求**；关上才整份交上去
//    （点「完成」/ 点别处 / 按 Esc / 切页 / 收起这一屏都算关上——卡一卸载就落一次，
//    见 `OutlineLinesCell.vue` 的 `onBeforeUnmount`）。整份覆盖的语义下逐勾发请求，
//    会让两次快速点击各自按手上那份旧清单算，后一次盖掉前一次；
// 2. **没动过就一个字都不发**：手上那份与库里那份一样时，关上不产生任何写（也不留痕）；
// 3. **回执取库里的那一份**：写完拿核心回的那份换掉手上那一行（与出场人物那一格同一条）；
// 4. **失败不假装**：存不下就报错、那一格保持原值，下次读回来还是库里的样子；
// 5. **合并只在 `attach` 一处**：表与标注是两条链，谁在这儿各拼一遍，迟早会拼出两套；
// 6. **换行别丢**：打开另一行的卡之前，先把手上这一行落下去（不然那几下会一声不吭地消失）。
import { ref, type Ref } from "vue";

import { nodeLines, nodeLinesSet, type OutlineRowDto } from "../api/outline.ts";
import { t } from "../locales/index.ts";
import { attachLines, markedLines, sameLines, toggleLine, type StoryLineCode } from "./outline-lines.ts";

export interface GridLinesOptions {
  /** 换掉手上那一行（回执从核心读回来） */
  replace: (row: OutlineRowDto) => void;
  /** 手上那一行 */
  rowOf: (node_id: number) => OutlineRowDto | undefined;
  /** 失败时把话交给表格统一显示 */
  report: (error: unknown) => void;
  busy: Ref<boolean>;
  /** 存下去之后闪一句回执（表格底下那一行提示） */
  justSaved: Ref<string>;
}

export interface GridLinesState {
  /** 现在开着勾选卡的是哪一行（同时只可能有一行） */
  openFor: Ref<number | null>;
  /** 手上那一份（**还没落库**）：卡上勾的就是它 */
  draft: Ref<string[]>;
  /** 读这本书的标注，并按节点 id 并到这些行上（表那一屏与标注那一屏在这儿合流） */
  attach: (work_id: number, rows: OutlineRowDto[]) => Promise<OutlineRowDto[]>;
  /** 打开某一行的卡（若开着别的行，先把那一行落下） */
  open: (node_id: number) => Promise<void>;
  /** 勾 / 取消一条：只动手上那份（**不发请求**） */
  toggle: (line: string) => void;
  /** 关上这一格：**手上那份与库里不一样才写一次**（整份覆盖） */
  close: () => Promise<void>;
  /** 收起 / 换书 / 切页：手上那份先落下 */
  reset: () => void;
}

export function useGridLines(deps: GridLinesOptions): GridLinesState {
  /** 现在打开勾选卡的是哪一行（同时只可能有一行） */
  const openFor = ref<number | null>(null);
  /** 手上那一份（**还没落库**）：卡上勾的就是它 */
  const draft = ref<string[]>([]);

  async function attach(work_id: number, rows: OutlineRowDto[]): Promise<OutlineRowDto[]> {
    return attachLines(rows, await nodeLines(work_id));
  }

  /**
   * 打开某一行的卡。
   *
   * 若已经开着别的行：**先把那一行落库**再换（不这么做，前一行勾的那几下会一声不吭地丢掉）。
   */
  async function open(node_id: number) {
    if (openFor.value !== null && openFor.value !== node_id) await close();
    const row = deps.rowOf(node_id);
    if (!row) return;
    openFor.value = node_id;
    draft.value = markedLines(row.lines);
  }

  /** 勾 / 取消一条：只动手上那份（**不发请求**）。 */
  function toggle(line: string) {
    draft.value = toggleLine(draft.value, line as StoryLineCode);
  }

  /**
   * 关上这一格：**手上那份与库里不一样才写一次**（整份覆盖）。
   *
   * 先把手上的卡收起来（`openFor = null`）再落库：写要过一次往返，这段时间里卡还开着，
   * 作者会以为"点了完成没反应"。
   */
  async function close() {
    const node_id = openFor.value;
    const wanted = draft.value;
    openFor.value = null;
    draft.value = [];
    if (node_id === null) return;
    const row = deps.rowOf(node_id);
    if (!row) return;
    if (sameLines(row.lines, wanted)) return; // 没动过：一个字都不发
    try {
      deps.busy.value = true;
      const saved = await nodeLinesSet(node_id, wanted);
      deps.replace({ ...row, lines: saved });
      deps.justSaved.value = t("grid.saved");
    } catch (error) {
      deps.report(error);
    } finally {
      deps.busy.value = false;
    }
  }

  /** 收起 / 换书 / 切页：手上那份先落下。 */
  function reset() {
    void close();
  }

  return { openFor, draft, attach, open, toggle, close, reset };
}
