// 磁盘 `.md` 镜像的**待定夺清单**：应用级一份，谁读谁写都看它。
//
// 为什么做成模块级（而不是每个组件各建一份）：设置面板那一块与启动时那个对话框说的是
// 同一件事，各拉各的状态迟早出现"面板说 0 条、对话框还列着 3 条"。
//
// 启动时看几眼：镜像第一趟对账是后台跑的（要把全部正文读一遍、再把磁盘比一遍），界面起来
// 那会儿往往还没跑完。所以隔一会儿看一次，看到了就把对话框请出来——**不阻塞写作**，
// 一次都没看到也不算错（可能是真的没有要定夺的事）。
//
// 这里只管"清单与状态"：处置动作在会话那边（它才拿得到编辑器与落盘控制器）。

import { ref } from "vue";

import { readMirrorStatus, type MirrorStatus } from "../api/mirror";

/** 启动后看几眼（第一趟对账通常在几百毫秒到几秒之间跑完）。 */
const FIRST_LOOK_MS = 1500;
const LOOK_EVERY_MS = 2500;
const LOOKS = 3;

const status = ref<MirrorStatus | null>(null);
const open = ref(false);
const error = ref<string | null>(null);

/** 再读一眼壳报上来的状态（很便宜：读的是壳里那份内存报告）。 */
export async function refreshMirror(): Promise<void> {
  try {
    status.value = await readMirrorStatus();
    error.value = null;
  } catch (failure) {
    // 读不到（壳没起镜像、IPC 出错）只留给设置面板去说，不打断写作
    error.value = failure instanceof Error ? failure.message : String(failure);
  }
}

/** 把清单请出来（启动时自动一次；设置面板里点「现在就看」也走这条）。 */
export function openMirrorReview(): void {
  open.value = true;
}

export function closeMirrorReview(): void {
  open.value = false;
}

/**
 * 启动时看几眼：一看到有要定夺的事就把对话框请出来，之后不再看。
 *
 * 为什么最多看三次而不是一直轮询：这事只在"作者在研墨之外动过文件"时才有，
 * 不是日常状态——一直轮询等于白烧电；三次覆盖第一趟对账跑完的那几秒。
 */
export function watchMirrorAtStartup(): void {
  let looks = 0;
  const look = async (): Promise<void> => {
    looks += 1;
    await refreshMirror();
    if ((status.value?.issues.length ?? 0) > 0) {
      open.value = true;
      return;
    }
    if (looks < LOOKS) {
      window.setTimeout(() => void look(), LOOK_EVERY_MS);
    }
  };
  window.setTimeout(() => void look(), FIRST_LOOK_MS);
}

/** 组件里取这份共享状态（返回的是同一批 ref，别在组件里再建一份）。 */
export function useMirrorReview() {
  return { status, open, error };
}
