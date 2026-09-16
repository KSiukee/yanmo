// 故事总纲那一页的**状态与命令编排**：读、写、与"换书跟着换"。
//
// 单独成文件的原因与别的 `*-panel.ts` 同一条：这一整块是"会发生什么"，
// `.vue` 那一份只管"长什么样"。
//
// 四条分寸：
// 1. **它自己跟着书走**：换书就重读（大纲表表头要摆这一段的摘要，而那张表可能先开）；
// 2. **失焦即存**：与大纲表逐格存同一条手感——一段长文没有一个"提交"的时刻；
// 3. **回执取库里的那一份**：显示的是核心读回来的文本，不是自己回显的；
// 4. **不猜**：存不下就留着作者手上的字并报错，绝不假装存上了。

import { ref, watch, type Ref } from "vue";

import { asError } from "../api/errors.ts";
import { setWorkStoryline, workStoryline } from "../api/storyline.ts";
import { t } from "../locales/index.ts";

export interface StorylinePanelOptions {
  workId: Ref<number | null>;
}

export interface StorylinePanelState {
  /** 手上这份文本（编辑框绑它） */
  text: Ref<string>;
  /** 库里存着的那一份（界面据此说"改没改过"） */
  saved: Ref<string>;
  busy: Ref<boolean>;
  /** 失败时那句已经渲染好的话（`CoreError` 走字典渲染） */
  errorText: Ref<string>;
  /** 刚存过（界面上闪一句回执） */
  justSaved: Ref<string>;
  load: () => Promise<void>;
  save: () => Promise<void>;
}

export function useStorylinePanel(deps: StorylinePanelOptions): StorylinePanelState {
  const text = ref("");
  const saved = ref("");
  const busy = ref(false);
  const errorText = ref("");
  const justSaved = ref("");

  function report(error: unknown) {
    errorText.value = asError(error).message;
  }

  async function load() {
    const work = deps.workId.value;
    if (!work) {
      text.value = "";
      saved.value = "";
      return;
    }
    try {
      busy.value = true;
      errorText.value = "";
      justSaved.value = "";
      const current = await workStoryline(work);
      text.value = current;
      saved.value = current;
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  /** 存一次（失焦与切页签都走这儿）。**没改过就不发**——免得白写一次库与留痕。 */
  async function save() {
    const work = deps.workId.value;
    if (!work || text.value === saved.value) return;
    try {
      busy.value = true;
      errorText.value = "";
      const stored = await setWorkStoryline(work, text.value);
      // 回执是库里真有的那一份。只更新 `saved`（不覆盖 `text`）：
      // 作者在等回执这段时间里又敲的字不能被吞掉。
      saved.value = stored;
      justSaved.value = t("storyline.saved");
    } catch (error) {
      report(error);
    } finally {
      busy.value = false;
    }
  }

  // 换书：重读（这一段属于那一本书）。**立即读一次**：大纲表表头要用它的摘要。
  watch(() => deps.workId.value, () => void load(), { immediate: true });

  return { text, saved, busy, errorText, justSaved, load, save };
}
