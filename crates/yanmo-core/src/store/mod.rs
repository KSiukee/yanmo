//! 存储层：**作品 / 节点树 / 正文 / 检索**的唯一读写入口。
//!
//! # 铁律
//!
//! 1. **单一真相源**：界面与增值模块都不直接写库，一律经 [`Store`]；
//! 2. **改数据的操作都留痕**：写操作追加 op-log（多端同步与创作留痕的底料）；
//! 3. **软删除**：删除只是打时间戳——回收站与"点错了"的撤销都靠它；
//! 4. **目录树不带正文**：[`node::NodeSummary`] 里**没有正文字段**——
//!    懒加载不是靠自觉，是靠类型（拿不到就是拿不到）。
//!
//! # 文件划分
//!
//! 一个领域一个文件（[`work`] / [`node`] + [`node_edit`] / [`content`] / [`search`] / [`export`] / [`trash`] / [`gap`] / [`appearance`]），
//! 各自只管自己的 SQL；公共句柄与留痕在 [`mod`] 与 [`device`]。
//! 节点的**读**与**编辑**特意分开：目录树查询和树结构变更的变化理由不一样。

mod appearance;
mod content;
mod device;
mod export;
mod gap;
mod node;
mod node_edit;
mod search;
mod session;
mod snapshot;
mod trash;
mod work;

pub use appearance::{Appearance, ResolvedAppearance};
pub use content::ContentStats;
pub use export::{ExportFormat, RenderedFile};
pub use gap::{ChapterGap, GapAnswer};
pub use node::{ChapterNeighbors, ChapterSummary, NodeSummary, SubtreeRollup};
pub use search::SearchHit;
pub use session::{EditorCursor, EditorTarget, SessionReport};
pub use snapshot::{SnapshotSummary, AUTO_SNAPSHOTS_KEPT};
pub use trash::{TrashEntry, TrashKind};
pub use work::ShelfEntry;

use std::path::Path;

use rusqlite::Connection;

use crate::db;
use crate::error::Result;

/// 节点树的最大深度。
///
/// 超过它说明数据已经坏了——**明确报错，而不是死循环或无限递归**。
/// 树的读与写都要用，所以放在这一层共用，免得两处各写一个上限（迟早会不一致）。
pub(super) const MAX_TREE_DEPTH: usize = 512;

/// 数据句柄：一条连接 + 本机设备标识。
pub struct Store {
    conn: Connection,
    device_id: String,
}

impl Store {
    /// 打开（或创建）数据库：校验环境 → 迁移 → 确保本机设备标识存在。
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_connection(db::open_ready(path)?)
    }

    /// 接管一条已经打开并迁移好的连接（命令行工具与测试用这条）。
    pub fn from_connection(conn: Connection) -> Result<Self> {
        let device_id = device::ensure(&conn)?;
        let mut store = Self { conn, device_id };
        // 字数预聚合回填（#128）：老库缺两个口径的列值。**只做一次**，标记在 settings 里；
        // 放在这里而不是 `open`，是为了让命令行、测试、壳走哪条入口都拿到一致的数据。
        store.backfill_node_counts()?;
        Ok(store)
    }

    /// 只读访问连接：查询、诊断、**写后读回校验**。
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// 可写访问：跨领域的事务用。
    ///
    /// 日常写入请优先走本层的领域方法——它们才会保证留痕与一致性。
    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    /// 本机设备标识（写入 op-log 时署名用）。
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// 追加一条变更日志。`entity` 用表名，`op` 用动词（create / rename / move / delete / write）。
    pub(crate) fn record(
        &self,
        entity: &str,
        entity_id: i64,
        op: &str,
        payload: serde_json::Value,
    ) -> Result<()> {
        db::append_op(&self.conn, &self.device_id, entity, entity_id, op, &payload.to_string())?;
        Ok(())
    }
}
