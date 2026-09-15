//! 迁移 v10：**延后队列**（`question_deferrals`）。
//!
//! ⚠️ 与 v1~v9 同样**一旦发布不可再改**。
//!
//! # 为什么是"一次延后一行"而不是卡上的几个列
//!
//! 三个理由：
//!
//! 1. **要数**：防死循环靠"同一问题累计延后几次"，一次一行天然就是一本次数账；
//! 2. **要留**：重出之后仍要知道"它当时在等什么、作者填了什么话"——真删了就没法回答；
//! 3. **要判**：待重出的记录要能被单独扫出来（`resolved_at IS NULL`，走索引），
//!    不用把整本书的卡都翻一遍。
//!
//! 列与语义：
//!
//! - `kind`：条件类型（`time` / `written` / `manual`），与核心的稳定码一致；
//! - `due_at_ms`：时间条件的时刻；别的类型为空；
//! - `anchor_node`：写到哪个节点；锚点被删（硬删）时置空——判条件时**置空按翻篇**处理
//!   （让作者删掉的那件事永远堵着一条问题，比"多问一次"更糟）；
//! - `note`：延后时作者自己填的那句「什么时候再问我」（**作者数据**，不是界面文案；
//!   核心不认识它，只负责把它原样还回去）；
//! - `created_at` / `resolved_at`：什么时候延后的、什么时候重出的。
//!
//! 外键：卡（`fragments`）级联删——卡真被清掉时它的延后记录没有留着的理由；
//! 锚点节点置空（见上）。

/// v10 的静态步骤：建表 + 建索引（都幂等，不需要 `prepare`）。
pub const STEPS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS question_deferrals (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        card_id     INTEGER NOT NULL REFERENCES fragments(id) ON DELETE CASCADE,
        kind        TEXT    NOT NULL,
        due_at_ms   INTEGER,
        anchor_node INTEGER REFERENCES nodes(id) ON DELETE SET NULL,
        note        TEXT    NOT NULL DEFAULT '',
        created_at  INTEGER NOT NULL,
        resolved_at INTEGER
    )",
    // 扫描"还等着"的那些：按卡取，且只认未解决的
    "CREATE INDEX IF NOT EXISTS idx_question_deferrals_open
        ON question_deferrals(card_id, resolved_at)",
];
