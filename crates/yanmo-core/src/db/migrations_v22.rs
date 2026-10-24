//! 迁移 v22：**模块注册表**——装了哪些模块、声明与授予了什么能力。
//!
//! ⚠️ 与 v1~v21 同样**一旦发布不可再改**。
//!
//! # 为什么注册表在 `yanmo.db`，不在 `modules.db`
//!
//! `modules.db` 是**模块自己的数据区**（它的命名空间、它的 schema 迁移）；
//! 注册表是**核心对模块的治理账**：装了谁、给了什么只读能力、冻结没有、认证没有。
//! 这本账必须与核心数据同生共死（备份、完整性检查、卸载后的残留判断都靠它），
//! 也不能因为某个模块的数据文件坏了就跟着读不出来。所以它进 `yanmo.db`。
//!
//! # 为什么整份清单存 JSON
//!
//! 清单是模块**自述**的完整内容（id / 名字 / 版本 / 能力声明 / 界面形态），
//! 存的是规范化 JSON。注册表要能完整回答"它当初声明了什么"，而不是被几列摘要
//! 改写过的版本。`version` 与 `protocol` 另存两列只为查询与审计方便；真源仍是 `manifest`。
//!
//! `granted` 是**用户授予**的能力（`{}` = 全拒），与声明的差别就是"声明了但没给"。

/// v22 的静态步骤：一张模块注册表。
pub const STEPS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS modules (
        id           TEXT PRIMARY KEY,
        manifest     TEXT    NOT NULL,
        version      TEXT    NOT NULL,
        protocol     INTEGER NOT NULL,
        certified    INTEGER NOT NULL DEFAULT 0,
        enabled      INTEGER NOT NULL DEFAULT 1,
        granted      TEXT    NOT NULL DEFAULT '{}',
        installed_at INTEGER NOT NULL,
        updated_at   INTEGER NOT NULL
    )",
];
