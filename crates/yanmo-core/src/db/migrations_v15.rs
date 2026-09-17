//! 迁移 v15：**磁盘 `.md` 镜像的账**——`mirror_state` 一张表。
//!
//! ⚠️ 与 v1~v14 同样**一旦发布不可再改**。
//!
//! # 这张表记什么
//!
//! 每个承载正文的节点一行：它现在对应镜像里的哪个文件（相对路径）、写下去的时候
//! **库里的正文指纹**与**文件字节指纹**分别是什么。
//!
//! 两把指纹各有各的用处，缺一不可：
//! - 正文指纹对"库这边变了没有"——**不读正文**就能判断镜像该不该重写（廉价的定期对账）；
//! - 文件指纹对"磁盘这边被人动过没有"——落盘前先比一次，**不是我们写的那份就一个字都不碰**，
//!   绝不把作者用记事本改过的东西静默覆盖掉。
//!
//! # 为什么不加外键
//!
//! 节点与作品**硬删**（清空回收站、彻底删除）之后，账还在：那份 `.md` 文件还躺在作者磁盘上，
//! 得有人按账去收（或按账告诉作者"这一份我动不了"）。外键级联会在删节点的那一刻把账悄悄抹掉，
//! 文件就成了没人认领的孤儿——与"不留孤儿"正好相反。收账由镜像对账自己做，不交给数据库。
//!
//! # 为什么 `conflict` 单独一列
//!
//! 外部改过的文件我们**不覆盖也不删**，只记一笔"这一份待作者定夺"。有了这一列，
//! 定期对账才不会把冲突当成"还没写完"而每两秒重扫一遍整本书的正文。

use rusqlite::Connection;

use crate::error::Result;

/// 建账表与它的按书索引。
///
/// ⚠️ 一律 `IF NOT EXISTS`：迁移**要能重跑**。真机上"升级到一半断电/被杀"就是版本号没推进、
/// 下次启动把这一条重跑一遍——那时表已经建好了，硬 `CREATE TABLE` 会当场报
/// "table mirror_state already exists"，作者看到的是"升级失败、打不开"（`tests/cards.rs`
/// 里那条回退版本号的用例正是这么把它抓出来的）。
pub const STEPS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS mirror_state (
        node_id       INTEGER PRIMARY KEY,
        work_id       INTEGER NOT NULL,
        relative_path TEXT    NOT NULL,
        body_hash     TEXT    NOT NULL,
        file_hash     TEXT    NOT NULL,
        size_bytes    INTEGER NOT NULL,
        written_at    INTEGER NOT NULL,
        conflict      INTEGER NOT NULL DEFAULT 0
    )",
    "CREATE INDEX IF NOT EXISTS idx_mirror_state_work ON mirror_state(work_id)",
    "CREATE INDEX IF NOT EXISTS idx_mirror_state_path ON mirror_state(relative_path)",
];

/// v15 没有静态之外的步骤：表是新建的，不需要先看库再决定。
pub fn prepare(_conn: &Connection) -> Result<Vec<String>> {
    Ok(Vec::new())
}
