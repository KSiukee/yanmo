//! 创作流碎片的读写：**作者自己记下来的那几种**（灵感速记 / 事件 / 口述段落）。
//!
//! 住在**碎片统一表**（`fragments`）里，与问题卡、答案同表不同 `frag_kind`
//! （另一种取值＝第二套存储，那是单一真相源最容易被破的地方）。同表也就同元数据：
//! `source` 说怎么记下的、`created_at` 说多新鲜、`used_count` 说用没用过、
//! `linked` 记着关联锚点——将来按"优先用新鲜碎片"爆灵感（大纲灵感那条线）要用的
//! 四样，这里天生都有，不必再补一列。
//!
//! # 三条分寸
//!
//! 1. **只管"记、看、删、捞回"**：这一层不猜碎片该怎么用（归位 / 勾连 / 排序是后面的事）；
//! 2. **软删**：删只是打时间戳，捞回就是把时间戳抹掉——与作品、章节同一条纪律；
//! 3. **不建问题卡与答案**：那两种归叩问那条线（[`FragmentKind::jotted`] 挡着），
//!    从这里凭空建一张，只会长出一张没人处置得了的孤儿卡。

use rusqlite::{params, OptionalExtension};
use serde_json::json;

use super::{work_alive, Store};
use crate::error::{codes, Error, Result};
use crate::model::{Fragment, FragmentCount, FragmentKind, InputSource};
use crate::time::now_millis;

/// 新建一条碎片要给的字段（其余由库里给默认）。
#[derive(Debug, Clone)]
pub struct NewFragment {
    pub work_id: i64,
    pub kind: FragmentKind,
    pub body: String,
    /// 怎么记下的：`typed` / `voice` / `mixed`（闭集，认不出来当场拒）。
    pub source: String,
    /// 关联锚点（`chapter:12` 这种）：记在哪一章下、将来到哪条线去找它都靠它。
    pub anchors: Vec<String>,
}

/// 随手记的碎片落库时的**中性状态**。
///
/// `fragments.status` 这一列是按 `frag_kind` 分家的：问题卡用它走六态生命周期，
/// 创作流碎片现在只有"记下了"这一种，所以给一个中性值，别把问题态混进来。
/// 等创作流那条线真要"用没用上"（落进正文 / 归到哪条线），再在这里长出它的语义。
const STATUS_JOTTED: &str = "pending";

/// 面板一屏最多读回多少条（再多的走筛选项，别一次全灌进界面）。
pub const FRAGMENTS_PER_BOARD: usize = 200;

const COLS: &str =
    "id, work_id, frag_kind, body, source, linked, derived_from, created_at";

/// 库里的一行原样读出来（`frag_kind` 还是字符串，认不认识交给 `into_fragment`）。
struct RawFragment {
    id: i64,
    work_id: Option<i64>,
    kind: String,
    body: String,
    source: String,
    linked: String,
    derived_from: Option<i64>,
    created_at: i64,
}

fn read_raw(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawFragment> {
    Ok(RawFragment {
        id: row.get(0)?,
        work_id: row.get(1)?,
        kind: row.get(2)?,
        body: row.get(3)?,
        source: row.get(4)?,
        linked: row.get(5)?,
        derived_from: row.get(6)?,
        created_at: row.get(7)?,
    })
}

/// 把一行变成一条碎片：**认不出的种类如实报错**，不猜一个"像那么回事"的。
fn into_fragment(raw: RawFragment) -> Result<Fragment> {
    Ok(Fragment {
        id: raw.id,
        // 与问题卡同一条口径：库里被手改成没有归属的按作品 0 算（如实显示，不假装有主）
        work_id: raw.work_id.unwrap_or(0),
        kind: FragmentKind::parse(&raw.kind)?,
        body: raw.body,
        source: raw.source,
        // 锚点那一列在库里是 JSON 数组文本；坏 JSON 按空数组读（这只是展示，
        // 不该因为一行坏数据让整屏读不出来——但要算它的人得走问题卡那条严格的路）
        anchors: serde_json::from_str(&raw.linked).unwrap_or_default(),
        derived_from: raw.derived_from,
        created_at: raw.created_at,
    })
}

/// 「这条碎片不存在」：只有一处说法，免得两种口径在同一条链上打架。
fn missing(id: i64) -> Error {
    Error::invalid_with(codes::FRAGMENT_NOT_FOUND, [("fragment_id", id.to_string())])
}

impl Store {
    /// 记一条碎片（写碎片 + 留痕，同一事务）。
    ///
    /// 校验都在写之前：正文不许空、种类必须是**随手记的那几种**、
    /// 输入方式走同一个闭集、书得还在。
    pub fn create_fragment(&mut self, new: &NewFragment, trigger: &str) -> Result<i64> {
        let body = new.body.trim();
        if body.is_empty() {
            return Err(Error::invalid(codes::FRAGMENT_BODY_EMPTY));
        }
        if !new.kind.jotted() {
            return Err(Error::invalid_with(
                codes::FRAGMENT_KIND_NOT_JOTTED,
                [("value", new.kind.as_str().to_string())],
            ));
        }
        // 输入方式与答案、灵感卡共用同一个闭集：认不出来的当场拒绝
        let source = InputSource::parse(&new.source)?.as_str();
        let now = now_millis();
        let tx = self.conn.transaction()?;
        if !work_alive(&tx, new.work_id)? {
            return Err(Error::invalid_with(
                codes::WORK_GONE,
                [("work_id", new.work_id.to_string())],
            ));
        }
        let anchors = serde_json::to_string(&new.anchors).unwrap_or_else(|_| "[]".to_string());
        tx.execute(
            "INSERT INTO fragments(work_id, frag_kind, body, source, status, importance,
                                   used_count, last_asked_at, linked, derived_from, auto_derived,
                                   template_key, created_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, 0.5, 0, NULL, ?6, NULL, 0, '', ?7, ?7)",
            params![
                new.work_id,
                new.kind.as_str(),
                body,
                source,
                STATUS_JOTTED,
                anchors,
                now
            ],
        )?;
        let id = tx.last_insert_rowid();
        Self::record_in(
            &self.device_id,
            &tx,
            "fragments",
            id,
            "create",
            json!({
                "kind": new.kind.as_str(),
                "source": source,
                "trigger": trigger,
            }),
        )?;
        tx.commit()?;
        Ok(id)
    }

    /// 取一条碎片（软删的不算）。
    pub fn fragment(&self, id: i64) -> Result<Fragment> {
        let sql = format!("SELECT {COLS} FROM fragments WHERE id = ?1 AND deleted_at IS NULL");
        let raw = self
            .conn
            .query_row(&sql, params![id], read_raw)
            .optional()?
            .ok_or_else(|| missing(id))?;
        into_fragment(raw)
    }

    /// 一本书里指定种类的碎片，**新的在前**。
    ///
    /// 一种一条查询（`kinds` 是个小闭集，最多五条），再在内存里按时间合并截断：
    /// 走 `idx_fragments_work(work_id, frag_kind, deleted_at)` 这个现成的索引，
    /// 也不必拼 `IN (?,?,…)` 那种字符串 SQL。
    pub fn fragments(
        &self,
        work_id: i64,
        kinds: &[FragmentKind],
        limit: usize,
    ) -> Result<Vec<Fragment>> {
        let sql = format!(
            "SELECT {COLS} FROM fragments
              WHERE work_id = ?1 AND frag_kind = ?2 AND deleted_at IS NULL
              ORDER BY created_at DESC, id DESC
              LIMIT ?3"
        );
        let mut out = Vec::new();
        for kind in kinds {
            let mut stmt = self.conn.prepare(&sql)?;
            let rows = stmt.query_map(params![work_id, kind.as_str(), limit as i64], read_raw)?;
            for row in rows {
                out.push(into_fragment(row?)?);
            }
        }
        // 合并之后再排一次：一屏是"最近记的几条"，不分种类
        out.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.id.cmp(&a.id))
        });
        out.truncate(limit);
        Ok(out)
    }

    /// 指定种类各有几条（**只数没删的**）——面板上那些筛选项的数字。
    pub fn fragment_counts(&self, work_id: i64, kinds: &[FragmentKind]) -> Result<Vec<FragmentCount>> {
        let mut out = Vec::with_capacity(kinds.len());
        for kind in kinds {
            let count: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM fragments
                  WHERE work_id = ?1 AND frag_kind = ?2 AND deleted_at IS NULL",
                params![work_id, kind.as_str()],
                |row| row.get(0),
            )?;
            out.push(FragmentCount {
                kind: *kind,
                count: count.max(0) as usize,
            });
        }
        Ok(out)
    }

    /// 删一条碎片：**只打时间戳**（捞回就是把时间戳抹掉）。
    ///
    /// 回执是**它删掉的那一条**——调用方（壳那一层）要靠 `work_id` 回一屏最新的面板，
    /// 删完再查就查不到了（软删在"取一条"那条路上算不存在）。
    pub fn delete_fragment(&mut self, id: i64, trigger: &str) -> Result<Fragment> {
        let fragment = self.fragment(id)?;
        let now = now_millis();
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE fragments SET deleted_at = ?1, updated_at = ?1
              WHERE id = ?2 AND deleted_at IS NULL",
            params![now, id],
        )?;
        Self::record_in(
            &self.device_id,
            &tx,
            "fragments",
            id,
            "delete",
            json!({ "kind": fragment.kind.as_str(), "trigger": trigger }),
        )?;
        tx.commit()?;
        Ok(fragment)
    }

    /// 捞回一条删掉的碎片，返回**捞回来之后库里那一条**。
    ///
    /// 已经在的（重复点"撤销"）**原样返回、不留痕**——那不是失败，没必要报错给作者看。
    pub fn restore_fragment(&mut self, id: i64, trigger: &str) -> Result<Fragment> {
        let sql = format!("SELECT {COLS} FROM fragments WHERE id = ?1");
        let raw = self
            .conn
            .query_row(&sql, params![id], read_raw)
            .optional()?
            .ok_or_else(|| missing(id))?;
        let fragment = into_fragment(raw)?;
        let now = now_millis();
        let tx = self.conn.transaction()?;
        let changed = tx.execute(
            "UPDATE fragments SET deleted_at = NULL, updated_at = ?1
              WHERE id = ?2 AND deleted_at IS NOT NULL",
            params![now, id],
        )?;
        if changed > 0 {
            Self::record_in(
                &self.device_id,
                &tx,
                "fragments",
                id,
                "restore",
                json!({ "kind": fragment.kind.as_str(), "trigger": trigger }),
            )?;
        }
        tx.commit()?;
        self.fragment(id)
    }
}
