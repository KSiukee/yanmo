// 模块运行的界面侧登记：**轮询状态 + 记开始/停止**。
//
// 拆出来只为让管理弹窗别再长成上帝文件；规则全在壳/核心，这里只维护界面快照。
import { ref } from "vue";

import {
  moduleStatus,
  type ModuleCard,
  type ModuleStatus,
} from "../api/modules";

export function useModuleRuns(onFinished: (status: ModuleStatus) => void) {
  const runs = ref<Record<string, ModuleStatus>>({});
  let timer: number | undefined;

  function ensurePolling() {
    if (timer !== undefined) return;
    timer = window.setInterval(() => {
      void poll();
    }, 800);
  }

  async function poll() {
    const ids = Object.keys(runs.value).filter((id) => runs.value[id]?.running);
    if (ids.length === 0) {
      if (timer !== undefined) {
        window.clearInterval(timer);
        timer = undefined;
      }
      return;
    }
    for (const id of ids) {
      try {
        const status = await moduleStatus(id);
        runs.value[id] = status;
        if (!status.running) onFinished(status);
      } catch {
        // 轮询失败不打断界面；下一次再试
      }
    }
  }

  function markStarted(moduleId: string) {
    runs.value[moduleId] = {
      known: true,
      running: true,
      stopped: false,
      timed_out: false,
      idle_seconds: 0,
      exit_code: null,
      emitted: [],
      stderr: "",
      error: null,
    };
    ensurePolling();
  }

  function markStopped(moduleId: string) {
    const current = runs.value[moduleId];
    if (current) runs.value[moduleId] = { ...current, running: false, stopped: true };
  }

  async function loadStatuses(cards: ModuleCard[]) {
    for (const card of cards) {
      try {
        const status = await moduleStatus(card.id);
        if (status.known) runs.value[card.id] = status;
      } catch {
        // 单个状态取不到不影响列表
      }
    }
    if (Object.values(runs.value).some((status) => status.running)) ensurePolling();
  }

  function isRunning(moduleId: string): boolean {
    return runs.value[moduleId]?.running === true;
  }

  function dispose() {
    if (timer !== undefined) {
      window.clearInterval(timer);
      timer = undefined;
    }
  }

  return { runs, isRunning, markStarted, markStopped, loadStatuses, ensurePolling, dispose };}
