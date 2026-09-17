// 启动时那句交代：**卡死（守护之心）优先于"上次没正常退出"**。
//
// 为什么要单独一件、还要单测：自动重载期间会话标记仍是"未正常退出"（会话本来就没结束），
// 两句话都会成立。不区分的话，**每一次自动复活都会蹦一句假警报**——崩溃提醒是作者唯一的
// 崩溃线索，误报几次就会被忽略（这条教训在核心的 `touch` 上已经吃过一次）。

import { t } from "../locales/index.ts";
import type { SessionNotice } from "../api/core.ts";

/**
 * 该对作者说哪一句（没什么可说的就是 `null`）。
 *
 * @param formatWhen 把"上次活动时间"格式化成一句人话（注入是为了测试不依赖本地时区）
 */
export function startupNotice(
  notice: SessionNotice,
  formatWhen: (millis: number) => string = (millis) => new Date(millis).toLocaleString(),
): string | null {
  if (notice.revive) {
    // 这一次多半就是自动重载回来的那一趟：说到底发生了什么，别让他以为是自己弄坏的
    return notice.revive.gave_up
      ? t("session.revive_failed", { count: notice.revive.attempt })
      : t("session.revive_notice", { count: notice.revive.attempt });
  }
  if (notice.unclean) {
    const when = notice.last_seen_at ? formatWhen(notice.last_seen_at) : t("session.time_unknown");
    return t("session.crash_notice", { when });
  }
  return null;
}
