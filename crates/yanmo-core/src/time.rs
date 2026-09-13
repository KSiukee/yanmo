//! 时间源——可注入的时钟。
//!
//! # 为什么要单独一个模块
//!
//! 研墨的头号差异化之一是「**创作留痕自证**」（证明这是真人写的、何时写的）。
//! 如果软件留有"可替换时间源"的能力，别人就能质疑时间戳可以伪造——**那会摧毁
//! 留痕的证据价值**。
//!
//! 所以本模块的铁律是：
//!
//! **注入能力只在测试构建里存在（`--features testing`），发布二进制里这段代码
//! 根本不存在。** 审计者可以直接证明"没有这个能力"，而不是靠读代码理解。
//!
//! 测试 [`tests::production_build_ignores_override`] 就是这条铁律的证据。

/// 读取系统时钟（unix 毫秒）。读不到时返回 0（调用方各自判断）。
fn system_now_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 当前时间（unix 毫秒）。
///
/// 发布构建：直接读系统时钟。
/// 测试构建：允许 `YANMO_TEST_CLOCK_MS` 覆盖，供时间相关的自动化测试使用。
pub fn now_millis() -> i64 {
    #[cfg(feature = "testing")]
    {
        if let Some(v) = test_clock_override() {
            return v;
        }
    }
    system_now_millis()
}

/// 测试专用时间覆盖（**仅 `testing` feature 编译进来**）。
#[cfg(feature = "testing")]
fn test_clock_override() -> Option<i64> {
    std::env::var("YANMO_TEST_CLOCK_MS")
        .ok()
        .and_then(|s| s.trim().parse::<i64>().ok())
}

/// 把 unix 毫秒拆成本地日历字段：`(年, 月, 日, 时, 分, 秒)`。
///
/// `offset_minutes` 是**本地时区相对 UTC 的偏移**（东八区 = +480）。核心不猜时区：
/// 拿得到时区的是界面（浏览器的 `getTimezoneOffset`）或壳，由调用方传进来——
/// 这样核心仍是纯函数、可单测，也不必为了一个日期格式化引一整套时区库。
///
/// 算法是标准的 civil-from-days（把"1970-01-01 起的天数"换算成年月日），
/// 对闰年与世纪闰年都对；范围到 9999 年足够用。
pub fn local_parts(millis: i64, offset_minutes: i32) -> (i64, u32, u32, u32, u32, u32) {
    let shifted = millis + i64::from(offset_minutes) * 60_000;
    // 向下取整的除法（负数时间戳也不能算歪）
    let days = shifted.div_euclid(86_400_000);
    let rest = shifted.rem_euclid(86_400_000);
    let (hour, minute, second) = (
        (rest / 3_600_000) as u32,
        ((rest / 60_000) % 60) as u32,
        ((rest / 1000) % 60) as u32,
    );
    let (year, month, day) = civil_from_days(days);
    (year, month, day, hour, minute, second)
}

/// 天数（1970-01-01 起）→ `(年, 月, 日)`。
///
/// 出处：Howard Hinnant 的 `civil_from_days`（公有领域的标准换算），自己抄一遍
/// 是为了不引日期库；公式里那个 `era` 分段正是"闰年规则"的紧凑写法。
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// 给人看的目录名时间戳：`20260913-0115`（本地时间）。
pub fn local_stamp(millis: i64, offset_minutes: i32) -> String {
    let (y, mo, d, h, mi, _) = local_parts(millis, offset_minutes);
    format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}")
}

/// 只到"哪一天"的本地日期：`2026-09-13`（账本里记"这一天备份过没有"）。
pub fn local_date(millis: i64, offset_minutes: i32) -> String {
    let (y, mo, d, _, _, _) = local_parts(millis, offset_minutes);
    format!("{y:04}-{mo:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_clock_is_sane() {
        // 2020-01-01 之后（防止时钟明显跑偏）
        assert!(system_now_millis() > 1_577_836_800_000);
    }

    #[test]
    fn clock_is_monotonic_enough() {
        let a = now_millis();
        let b = now_millis();
        assert!(b >= a);
    }

    /// ★ 铁律证据：**非 testing 构建下，环境变量无法影响时间源**。
    ///
    /// 这就是"发布二进制里不存在这个能力"的自动化证明——审计者不必读代码。
    #[cfg(not(feature = "testing"))]
    #[test]
    fn production_build_ignores_override() {
        std::env::set_var("YANMO_TEST_CLOCK_MS", "1000");
        let got = now_millis();
        std::env::remove_var("YANMO_TEST_CLOCK_MS");
        assert!(
            got > 1_577_836_800_000,
            "发布构建不得受 YANMO_TEST_CLOCK_MS 影响（得到 {got}）"
        );
    }

    /// 反向证据：**testing 构建下注入必须生效**（否则时间相关的测试无法构造场景）。
    #[cfg(feature = "testing")]
    #[test]
    fn testing_build_honours_override() {
        std::env::set_var("YANMO_TEST_CLOCK_MS", "1234567890123");
        let got = now_millis();
        std::env::remove_var("YANMO_TEST_CLOCK_MS");
        assert_eq!(got, 1_234_567_890_123);
    }

    #[test]
    fn civil_dates_handle_epoch_leap_years_and_offsets() {
        // 期望值不是手算的：用带时区库的独立实现（Python datetime）算好再抄进来
        assert_eq!(local_parts(0, 0), (1970, 1, 1, 0, 0, 0));
        // 1787000100000 = 2026-08-17 20:55:00 UTC
        assert_eq!(local_parts(1_787_000_100_000, 0), (2026, 8, 17, 20, 55, 0));
        // 东八区：同一条时间戳换成 +480 分钟 = 2026-08-18 04:55
        assert_eq!(local_parts(1_787_000_100_000, 480), (2026, 8, 18, 4, 55, 0));
        // 闰日 2024-02-29 12:00
        assert_eq!(local_parts(1_709_208_000_000, 0), (2024, 2, 29, 12, 0, 0));
        // 世纪闰年 2000-02-29 有这一天；1900/2100 的 3 月 1 日证明它们不是闰年
        assert_eq!(local_parts(951_782_400_000, 0), (2000, 2, 29, 0, 0, 0));
        assert_eq!(local_parts(-2_203_891_200_000, 0), (1900, 3, 1, 0, 0, 0), "1970 之前（负数时间戳）也要算得对");
        assert_eq!(local_parts(4_107_542_400_000, 0), (2100, 3, 1, 0, 0, 0));
    }

    #[test]
    fn stamps_are_stable_and_sortable() {
        assert_eq!(local_stamp(1_787_000_100_000, 0), "20260817-2055");
        assert_eq!(local_date(1_787_000_100_000, 0), "2026-08-17");
        assert_eq!(local_stamp(1_787_000_100_000, 480), "20260818-0455");
        // 字面量可排序 = 按时间排序（保留滚动靠它挑"最旧的一份"）
        let a = local_stamp(1_787_000_100_000, 0);
        let b = local_stamp(1_787_000_160_000, 0);
        assert!(a < b);
    }
}
