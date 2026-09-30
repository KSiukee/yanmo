// 「什么时候值得问一句」的纯逻辑：切章那一刻该按哪个时机问、问的是**哪一章**。
//
// 单独成文件是为了能被测：`question-push.ts` 那边要 import 真命令（`api/question.ts`），
// 而 `node --test` 跑的是带类型剥离的 ESM——那一串运行时 import 拉着 Tauri 网关，测试进不去。
//
// 「时机」与「它说的那一章」必须配对——`chapter_done` 说的是**刚离开**的那一章（它刚被写完），
// 不是切过去的新章。配错方向会出两种毛病：把"这一章写完了"挂在空章上问（核心的「空章不问」
// 判据会拦掉，于是「写完一章 → 切到空章」这条最常见的路一次都问不出来），
// 或者把问题记到不相干的那一章名下。

/** 为什么现在开口（进留痕的 trigger，事后看得出是哪条时机触发的）。 */
export type PushReason = "new_chapter" | "idle" | "chapter_done";

/** 一章的"当前样子"里与时机有关的两样：是哪一章、写了多少字。 */
export interface ChapterSpot {
  node: number | null;
  chars: number;
}

/**
 * 切章那一刻的意图：该问就问、时机与章节配对；不该问就 `null`。
 *
 * 三条判据（与 `question-push.ts` 的头注同一件事）：
 * - 同一章里字数变了不算切章；
 * - 离开的那一章**有正文** ⇒ 刚写完一章（说的是**它**）；
 * - 新到的那一章是空的 ⇒ 开新章（说的是**新到的它**）；
 * - 其余（切到一章已经有正文的地方）不问——那既不是收工，也不是开头。
 */
export function switchPush(left: ChapterSpot, arrived: ChapterSpot): {
  reason: PushReason;
  about: number | null;
} | null {
  if (left.node === arrived.node) return null;
  if (left.node !== null && left.chars > 0) {
    return { reason: "chapter_done", about: left.node };
  }
  if (arrived.chars === 0) {
    return { reason: "new_chapter", about: arrived.node };
  }
  return null;
}
