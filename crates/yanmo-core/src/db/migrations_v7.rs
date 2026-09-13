//! 迁移 v7：**每日码字流水**（`writing_days`）——「今日进度 / 码字日历」的账本。
//!
//! ⚠️ 与 v1~v6 同样**一旦发布不可再改**。
//!
//! # 为什么是一张独立的流水表
//!
//! `nodes` 上存的是**此刻的字数**（预聚合，供目录树一眼看），它答不了
//! "我昨天写了多少"——那需要一个"每一天各记一笔"的账本。所以另起一表：
//! 主键 `(day, work_id)`，三口径各一列，值是**当天的净增减**
//! （删字记负，与作者眼前看到的字数变化一致）。
//!
//! # 为什么三口径都存
//!
//! 口径是**显示偏好**（作者随时可切）。账本只按一个口径记的话，作者一换口径，
//! 历史那几天就跟着变意思了。三列一起记，历史永远是同一件事的三种读法。
//!
//! # 为什么静态步骤就够（不需要 `prepare`）
//!
//! 本版只有建表与建索引，两者都带 `IF NOT EXISTS`——重跑不会撞车，
//! 不需要像 v3~v6 那样先查 `PRAGMA table_info` 再决定（迁移铁律第 3 条）。

/// v7 的静态步骤：建表 + 建索引（都幂等）。
pub const STEPS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS writing_days (
        day            TEXT    NOT NULL,
        work_id        INTEGER NOT NULL,
        chars          INTEGER NOT NULL DEFAULT 0,
        chars_no_punct INTEGER NOT NULL DEFAULT 0,
        words          INTEGER NOT NULL DEFAULT 0,
        updated_at     INTEGER NOT NULL,
        PRIMARY KEY (day, work_id)
    )",
    // 主键索引管"按天取"（日历）；这本账还要"按书取"（某本书的连续天数），另建一条
    "CREATE INDEX IF NOT EXISTS idx_writing_days_work ON writing_days(work_id, day)",
];
