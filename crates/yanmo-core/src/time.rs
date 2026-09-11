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
}
