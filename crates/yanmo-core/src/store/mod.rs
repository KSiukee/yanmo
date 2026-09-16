//! 存储层：**作品 / 节点树 / 正文 / 检索**的唯一读写入口。
//!
//! # 铁律
//!
//! 1. **单一真相源**：界面与增值模块都不直接写库，一律经 [`Store`](crate::store::Store)；
//! 2. **改数据的操作都留痕**：写操作追加 op-log（多端同步与创作留痕的底料）；
//! 3. **软删除**：删除只是打时间戳——回收站与"点错了"的撤销都靠它；
//! 4. **目录树不带正文**：[`NodeSummary`](crate::store::NodeSummary) 里**没有正文字段**——
//!    懒加载不是靠自觉，是靠类型（拿不到就是拿不到）。
//!
//! # 文件划分
//!
//! 一个领域一个文件（`work` / `node` + `node_edit` / `content` / `search` / `export` / `trash` /
//! `appearance` / `card` + `card_move`），各自只管自己的 SQL；公共句柄与留痕在 `mod` 与 `device`。
//! 节点的**读**与**编辑**特意分开：目录树查询和树结构变更的变化理由不一样。
//! 问题卡同理：**读写**与**状态迁移**分开（迁移要留可核对的证据，理由不一样）。

mod appearance;
mod backup;
mod card;
mod card_move;
mod card_row;
mod cast;
mod defer_condition;
mod content;
mod device;
mod draft;
mod entity_card;
mod export;
mod foreshadow;
mod fragment;
mod import;
mod node;
mod naming;
mod node_edit;
mod numbering;
mod outline_dismiss;
mod outline_grid;
mod outline_paste;
mod outline_scan;
mod question_answer;
mod question_land;
mod question_defer;
mod question_dispose;
mod question_inspire;
mod question_defer_write;
mod question_pool;
mod question_push;
mod question_select;
mod question_weights;
mod relocate;
mod restore;
mod restore_packages;
mod restore_swap;
mod scene_card;
mod search;
mod session;
mod snapshot;
mod trash;
mod volume;
mod work;
mod writing;

pub use appearance::{Appearance, ResolvedAppearance};
pub use backup::{
    gaps_for, has_other_volume, ledger_summary, read_ledger, read_manifest, BackupConfig,
    BackupLedger, BackupManifest, BackupReport, BackupRequest, BackupTarget, BackupVerify,
    LedgerEntry, TargetOutcome, WorkStamp,
};
pub use card::KIND_QUESTION;
pub use card_move::CardEvent;
pub use cast::CastMember;
pub use content::ContentStats;
pub use export::{ExportFormat, RenderedFile};
pub use fragment::{FragmentEdit, NewFragment, FRAGMENTS_PER_BOARD};
pub use outline_grid::OutlineRow;
pub use outline_paste::{OutlineCell, MAX_PASTE_CELLS};
pub use draft::{parse_work_json, DraftScale, NodeDraft, StampMismatch, WorkDraft};
pub use import::{find_drafts, manifest_near, ImportReport};
pub(crate) use export::normalize;
pub use naming::NamingRewrite;
pub use node::{ChapterNeighbors, ChapterSummary, NodeSummary, SubtreeRollup};
pub use relocate::{copy_dir, verify_same_scale, Relocation};
pub use restore::RestorePreview;
pub use restore_packages::{list_packages, scan_packages, PackageBrief};
pub use restore_swap::{swap_in, RestoreOutcome, KEEP_FOLDER};
pub use question_answer::Answer;
pub use question_land::{LandReceipt, RoundItem};
pub use question_defer::Deferral;
pub use question_dispose::CooledCard;
pub use question_inspire::Inspiration;
pub use question_push::PushOutcome;
pub use question_select::SelectedQuestion;
pub use question_weights::TemplateLearning;
pub use search::SearchHit;
pub use session::{EditorCursor, EditorTarget, SessionReport};
pub use snapshot::{SnapshotSummary, AUTO_SNAPSHOTS_KEPT};
pub use trash::{TrashEntry, TrashKind};
pub use volume::{CloseReceipt, DissolveReceipt};
pub use work::ShelfEntry;
pub use writing::WritingDay;

use std::path::Path;

use rusqlite::{params, Connection, Transaction};

use crate::db;
use crate::error::Result;

/// 节点树的最大深度（**层数**，根节点算第 1 层）。
///
/// # 为什么是 64，不是 512
///
/// 这个数同时管三件事，所以只能有一个：
/// ① **写入口守门**（新建 / 移动超过就拒绝，见 [`super::node_edit`]）；
/// ② **读路径兜底**（祖先链 / 子树汇总碰到就越限报错，见 [`Store::node_ancestors`]，
///    [`Store::subtree_rollup`]）——坏数据不许让它转到天荒地老，更不许静默少算；
/// ③ **递归走法的深度预算**：导出（分章 txt / 单文件 json）与编译都是按层递归的。
///
/// 原来定的 512 是当"坏数据哨兵"用的，**比代码实际走得动的深度还大**：
/// 2026-09-14 由外部演练台（独立工具仓的 `deep-tree` 那一场）实测——调试构建一路建到
/// 512 层时 `export --format json` **栈溢出**（`thread 'main' has overflowed its stack`）；
/// 发布构建 512 层能过，但调试构建在 384 层就崩、256 层才稳。而写作软件的大纲
/// （卷 → 章 → 节 → 场景卡）通常 2~4 层，512 这个数从来没有产品上的理由。
/// 现在收到 64：离调试构建的崩点（384）还有 ≥4 倍余量，也远宽于任何真实书。
pub(super) const MAX_TREE_DEPTH: usize = 64;

/// 「太深了」这条错：参数只有上限一个，话怎么说留给界面。
pub(super) fn too_deep() -> crate::error::Error {
    crate::error::Error::invalid_with(
        crate::error_codes::codes::TREE_TOO_DEEP,
        [("max", MAX_TREE_DEPTH.to_string())],
    )
}

/// 这本书在不在（**软删的也算不在**）：往一本已经进回收站的书里塞东西没有意义。
///
/// 放在这里而不是某一个领域文件里：问题卡与创作流碎片都要问同一句话，
/// 两处各判一遍就有两种口径（一个把软删当"在"，一个不当）那天。
pub(super) fn work_alive(conn: &Connection, work_id: i64) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM works WHERE id = ?1 AND deleted_at IS NULL)",
        params![work_id],
        |r| r.get(0),
    )?)
}

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
        // 字数预聚合回填：老库缺两个口径的列值。**只做一次**，标记在 settings 里；
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

    /// 追加一条变更日志：`entity` 用表名，`op` 用动词（create / rename / move / delete / write）。
    ///
    /// **留痕必须与它记录的那次写入同一个事务**——提交之后再留痕，一旦留痕失败
    /// 就会把**已经落库的操作**报成失败，作者据此重试会做出第二本书 / 第二次改名
    /// （2026-09-15 代码质量评审：中等 6）。所以这里只有一个在事务里留痕的入口。
    ///
    /// 它是关联函数而不是方法（不收 `&self`）：调用方正持有 `self.conn` 的事务借用，
    /// 再借整个 `self` 会冲突——设备标识由调用方按字段借出来即可
    /// （`&self.device_id` 与 `self.conn` 是不相交的两个字段）。
    pub(crate) fn record_in(
        device_id: &str,
        tx: &Transaction<'_>,
        entity: &str,
        entity_id: i64,
        op: &str,
        payload: serde_json::Value,
    ) -> Result<()> {
        db::append_op(tx, device_id, entity, entity_id, op, &payload.to_string())?;
        Ok(())
    }
}
