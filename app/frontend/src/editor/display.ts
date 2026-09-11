// 给人看的格式：字数怎么念、时间怎么念。
//
// 单独一小块的理由：这些是**纯展示口径**，改了它不该牵动目录树或书架的逻辑；
// 反过来，谁也不该为了显示一个数字去 import 别人的状态机。

/** 字数给人看：一万以下报原数，一万以上报「x.x万」（一位小数）。 */
export function formatWords(words: number): string {
  if (words < 10000) return String(words);
  return `${Math.round(words / 1000) / 10}万`;
}

/** 最近打开时间给人看：今天 / 昨天 / N 天前 / 具体日期；从没打开过就直说。 */
export function formatWhen(ms: number | null, now: number = Date.now()): string {
  if (ms === null) return "还没打开过";
  const days = Math.floor((now - ms) / 86_400_000);
  if (days <= 0) return "今天";
  if (days === 1) return "昨天";
  if (days < 30) return `${days} 天前`;
  const date = new Date(ms);
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${date.getFullYear()}-${month}-${day}`;
}
