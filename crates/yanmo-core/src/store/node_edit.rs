//! 节点树的**编辑**：建 / 改名 / 移动 / 软删除。
//!
//! - 同级排序是**密集序号**（0,1,2…）：每次移动后重排，幂等、无空洞、结果可复现；
//! - 移动前做**成环检测**：不允许把节点移进自己的子孙——树坏掉比报错难查得多；
//! - 软删除连带整棵子树，正文与历史都留着（回收站与"删错了"的撤销靠它）；
//! - 读在 [`super::node`]，两边分开，改编辑不会牵动目录树查询。

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::json;

use super::{Store, MAX_TREE_DEPTH};
use crate::error::{codes, Error, Result};
use crate::model::NodeKind;
use crate::time::now_millis;

/// 同父下的节点 id，按现有顺序。
fn sibling_ids(conn: &Connection, work_id: i64, parent_id: Option<i64>) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM nodes
         WHERE work_id = ?1 AND parent_id IS ?2 AND deleted_at IS NULL
         ORDER BY sort_order, id",
    )?;
    let rows = stmt.query_map(params![work_id, parent_id], |r| r.get(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 把同父节点重排成密集序号；`moved` 指定要挪到 `index` 位置的节点。
///
/// 删除与恢复也要用它（删完收洞、恢复时按原位锚回），所以对同层的模块开放。
pub(super) fn renumber(
    conn: &Connection,
    work_id: i64,
    parent_id: Option<i64>,
    moved: Option<(i64, usize)>,
) -> Result<()> {
    let mut ids = sibling_ids(conn, work_id, parent_id)?;
    if let Some((id, index)) = moved {
        ids.retain(|x| *x != id);
        ids.insert(index.min(ids.len()), id);
    }
    for (position, id) in ids.iter().enumerate() {
        conn.execute(
            "UPDATE nodes SET sort_order = ?1 WHERE id = ?2",
            params![position as i64, id],
        )?;
    }
    Ok(())
}

/// `candidate` 是否在 `ancestor` 的子树里。
fn is_descendant(conn: &Connection, candidate: i64, ancestor: i64) -> Result<bool> {
    let mut current = candidate;
    for _ in 0..MAX_TREE_DEPTH {
        let parent: Option<i64> = conn
            .query_row(
                "SELECT parent_id FROM nodes WHERE id = ?1",
                params![current],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        match parent {
            Some(p) if p == ancestor => return Ok(true),
            Some(p) => current = p,
            None => return Ok(false),
        }
    }
    Err(Error::invalid(codes::TREE_CYCLE_SUSPECTED))
}

/// 默认名的"前缀 / 后缀"：`第 12 章` 这种编号的骨架（场景卡没有"第…章"的说法，给个朴素名字）。
// i18n-allow-begin: 这张表产出的是**会写进库的默认名**（作者的数据，可随时改），不是界面文案
fn naming(kind: NodeKind) -> (&'static str, &'static str) {
    match kind {
        NodeKind::Volume => ("第", "卷"),
        NodeKind::Chapter => ("第", "章"),
        NodeKind::Section => ("第", "节"),
        NodeKind::Piece => ("第", "篇"),
        NodeKind::Scene => ("场景卡", ""),
    }
}
// i18n-allow-end

/// 从标题里认出编号：阿拉伯数字与中文数字都认（作者手打的「第一章」也算数）。
///
/// **只管开头**：编号后面挂着章名是常态（`第6章 灯`、`第十二章（上）`），不能因为后面有字
/// 就认不出来。认不出的后果不是"少认一个号"，而是取号退回**按数量算**——于是作者连点几次「+」
/// 就出现重复章号（真踩过：第二卷里已有 第6~10 章，新章却从「第6章」重新数起，
/// 看上去像章号倒着长）。
///
/// 认不出编号的名字（`序章` / `楔子` / `第一次见面`）一概不猜，也就不会误判成编号。
pub(super) fn parse_serial(title: &str, kind: NodeKind) -> Option<i64> {
    let (prefix, suffix) = naming(kind);
    let rest = title.trim().strip_prefix(prefix)?;
    let number = if suffix.is_empty() {
        // 场景卡没有"第…卡"这种骨架：整串都得是数字才算，免得把「场景卡牌」当成编号
        rest
    } else {
        // 数字与后缀之间不许夹别的字：`第1-2章`、`第一次见面` 都不算
        let at = rest.find(suffix)?;
        &rest[..at]
    };
    let number = normalize_digits(number.trim());
    number
        .parse::<i64>()
        .ok()
        .or_else(|| parse_cn_number(&number))
        .filter(|serial| *serial > 0)
}

/// 全角数字 → 半角：中文输入法下打出来的编号常常是 `１０`（U+FF10…），不归一就整串认不出。
fn normalize_digits(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\u{FF10}'..='\u{FF19}' => {
                char::from_u32(c as u32 - 0xFF10 + '0' as u32).unwrap_or(c)
            }
            _ => c,
        })
        .collect()
}

/// 认中文数字：`一` / `十` / `十二` / `二十三` / `七十八` / `一百二十` / `两千零五`，
/// 以及**大写与繁体**那一套（`壹` `貳` `參` `拾` `佰` `仟` `兩`）——作者怎么写都得认出来。
///
/// 规矩：单位（十/百/千）必须**从大到小**出现。`十十`、`百十`、`一百百` 这种瞎写的组合一律
/// 不认（返回 `None` 退回"没编号"），宁可不认，也不能猜出一个错的号——猜错的号会让目录顺序与
/// 章号对不上，那正是"随便试试就出 bug"的来源。
///
/// 认不出来时最坏也只是退化成 `第一章` 那类"没编号"的处理（不猜、不重号），不会崩。
fn parse_cn_number(text: &str) -> Option<i64> {
    /// 这个字代表几；第二个返回值表示"是不是单位"。
    fn value_of(c: char) -> Option<(i64, bool)> {
        Some(match c {
            '零' | '〇' => (0, false),
            '一' | '壹' => (1, false),
            '二' | '两' | '兩' | '贰' | '貳' => (2, false),
            '三' | '叁' | '参' | '參' => (3, false),
            '四' | '肆' => (4, false),
            '五' | '伍' => (5, false),
            '六' | '陆' | '陸' => (6, false),
            '七' | '柒' => (7, false),
            '八' | '捌' => (8, false),
            '九' | '玖' => (9, false),
            '十' | '拾' => (10, true),
            '百' | '佰' => (100, true),
            '千' | '仟' => (1000, true),
            _ => return None,
        })
    }
    if text.is_empty() {
        return None;
    }
    let mut total = 0i64; // 已经结算完的部分
    let mut current = 0i64; // 手上这个数字
    let mut last_unit = i64::MAX;
    for c in text.chars() {
        let (value, is_unit) = value_of(c)?;
        if !is_unit {
            current = value;
            continue;
        }
        if value >= last_unit {
            return None; // 单位没从大到小：不认
        }
        last_unit = value;
        total += if current == 0 { value } else { current * value };
        current = 0;
    }
    Some(total + current)
}

/// 标题留空时的默认名——**按同层同类型取号**（卷 / 章 / 节 / 篇），不按全书。
///
/// 取这一层**已用过的最大号 + 1**：删掉中间那一章之后新建，不会再算出"第3章"却与还在的第3章撞名；
/// 一个编号都认不出来（作者全用了自起的名字）就退回"同层同类现有几章 + 1"。
///
/// **空档不在这里补**：要不要补上删掉的那一章，由界面在点「+」时问一句
/// （见 `super::gap`），作者说补才用那个号——**不猜**。
fn default_title(
    conn: &Connection,
    work_id: i64,
    parent_id: Option<i64>,
    kind: NodeKind,
) -> Result<String> {
    let mut stmt = conn.prepare(
        "SELECT title FROM nodes
         WHERE work_id = ?1 AND parent_id IS ?2 AND node_kind = ?3 AND deleted_at IS NULL",
    )?;
    let rows = stmt.query_map(params![work_id, parent_id, kind.as_str()], |r| r.get::<_, String>(0))?;
    let mut max_used = 0;
    let mut count = 0;
    for row in rows {
        count += 1;
        if let Some(serial) = parse_serial(&row?, kind) {
            max_used = max_used.max(serial);
        }
    }
    let (prefix, suffix) = naming(kind);
    let serial = if max_used > 0 { max_used + 1 } else { count + 1 };
    Ok(format!("{prefix}{serial}{suffix}"))
}

impl Store {
    /// 在指定位置新建节点。`parent_id = None` 表示根级。
    ///
    /// **标题留空 = 按同层取号自动命名**（`第N章` 之类）——建节点的入口只有这一条，
    /// 所以"默认名怎么取"只有这一份实现，谁调都一致。
    pub fn create_node(
        &mut self,
        work_id: i64,
        parent_id: Option<i64>,
        kind: NodeKind,
        title: &str,
    ) -> Result<i64> {
        super::work::ensure_alive(&self.conn, work_id)?;
        if let Some(parent) = parent_id {
            self.ensure_node_in_work(parent, work_id)?;
        }
        let title = title.trim();
        let title = if title.is_empty() {
            default_title(&self.conn, work_id, parent_id, kind)?
        } else {
            title.to_string()
        };
        let now = now_millis();
        let tx = self.conn.transaction()?;
        let next: i64 = tx.query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM nodes
             WHERE work_id = ?1 AND parent_id IS ?2 AND deleted_at IS NULL",
            params![work_id, parent_id],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO nodes(work_id, parent_id, node_kind, title, sort_order, created_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![work_id, parent_id, kind.as_str(), title, next, now],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;

        self.record(
            "nodes",
            id,
            "create",
            json!({ "work_id": work_id, "parent_id": parent_id, "kind": kind.as_str() }),
        )?;
        Ok(id)
    }

    /// 改名（标题经触发器同步进检索索引）。
    pub fn rename_node(&mut self, id: i64, title: &str) -> Result<()> {
        let affected = self.conn.execute(
            "UPDATE nodes SET title = ?1, updated_at = ?2 WHERE id = ?3 AND deleted_at IS NULL",
            params![title.trim(), now_millis(), id],
        )?;
        if affected == 0 {
            return Err(Error::invalid_with(codes::NODE_GONE, [("node_id", id.to_string())]));
        }
        self.record("nodes", id, "rename", json!({ "title": title.trim() }))
    }

    /// 写一章的"一句话"（投稿包的大纲要用它）。
    ///
    /// 存的是作者的原话，**一个字的处理都不做**（不 trim、不分句）——作者写了什么就是什么；
    /// 日志里只记字数，不把整段话抄进变更留痕。
    pub fn set_node_summary(&mut self, id: i64, summary: &str) -> Result<()> {
        let affected = self.conn.execute(
            "UPDATE nodes SET summary = ?1, updated_at = ?2 WHERE id = ?3 AND deleted_at IS NULL",
            params![summary, now_millis(), id],
        )?;
        if affected == 0 {
            return Err(Error::invalid_with(codes::NODE_GONE, [("node_id", id.to_string())]));
        }
        self.record("nodes", id, "set_summary", json!({ "chars": summary.chars().count() }))
    }

    /// 移动节点到新父级的第 `index` 位（越界会夹到末尾），并把两侧同级重排成密集序号。
    pub fn move_node(&mut self, id: i64, new_parent: Option<i64>, index: usize) -> Result<()> {
        let work_id = self.node_work(id)?;
        let old_parent: Option<i64> = self
            .conn
            .query_row("SELECT parent_id FROM nodes WHERE id = ?1", params![id], |r| r.get(0))?;

        if let Some(parent) = new_parent {
            self.ensure_node_in_work(parent, work_id)?;
            if parent == id || is_descendant(&self.conn, parent, id)? {
                return Err(Error::invalid(codes::TREE_MOVE_INTO_DESCENDANT));
            }
        }

        let now = now_millis();
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE nodes SET parent_id = ?1, updated_at = ?2 WHERE id = ?3",
            params![new_parent, now, id],
        )?;
        renumber(&tx, work_id, new_parent, Some((id, index)))?;
        if old_parent != new_parent {
            renumber(&tx, work_id, old_parent, None)?;
        }
        tx.commit()?;

        self.record(
            "nodes",
            id,
            "move",
            json!({ "parent_id": new_parent, "index": index }),
        )
    }

    /// 软删除节点**及其整棵子树**，返回受影响的节点数。
    pub fn soft_delete_node(&mut self, id: i64) -> Result<usize> {
        // 删之前先问清"它属于哪本书、挂在谁下面"——删完这两样就问不出来了
        let work_id = self.node_work(id)?;
        let parent_id: Option<i64> = self
            .conn
            .query_row("SELECT parent_id FROM nodes WHERE id = ?1", params![id], |r| r.get(0))
            .optional()?
            .flatten();

        let tx = self.conn.transaction()?;
        let affected = tx.execute(
            "WITH RECURSIVE sub(id) AS (
                 SELECT id FROM nodes WHERE id = ?1
                 UNION ALL
                 SELECT n.id FROM nodes n JOIN sub ON n.parent_id = sub.id
             )
             UPDATE nodes SET deleted_at = ?2
             WHERE id IN (SELECT id FROM sub) AND deleted_at IS NULL",
            params![id, now_millis()],
        )?;
        if affected == 0 {
            return Err(Error::invalid_with(codes::NODE_GONE, [("node_id", id.to_string())]));
        }
        // 同级会留一个洞：**当场收成密集序号**。这样"同级序号是密集的"这条不变量
        // 在任何时候都成立，恢复时也才有一个稳定的"原来在第几位"可锚。
        renumber(&tx, work_id, parent_id, None)?;
        tx.commit()?;

        self.record("nodes", id, "delete_subtree", json!({ "affected": affected }))?;
        Ok(affected)
    }
}

impl Store {
    /// 在当前章**后面**插一章（同级、紧随其后）。
    ///
    /// 目录树接上之前，这是"再多写一章"的唯一入口；目录树的「+」将来走同一条路。
    /// 先追加到末尾再移到当前章之后——排序靠 [`Store::move_node`] 的密集序号，天然幂等。
    /// 按编号算插入位置：插在"最后一个编号比它小的同层兄弟"之后（最小的号放最前）。
    ///
    /// 返回 `None` = 这个标题里没有编号，调用方自己决定落哪儿。
    ///
    /// `ignore` 传"正要放进去的那一个"：它已经在同层名单里时（刚建出来、刚从回收站捞回来），
    /// 得先把它自己摘出去再数位置，不然算出来的下标会差一位。
    pub(super) fn index_by_serial(
        &self,
        title: &str,
        work_id: i64,
        parent: Option<i64>,
        kind: NodeKind,
        ignore: Option<i64>,
    ) -> Result<Option<usize>> {
        let Some(serial) = parse_serial(title, kind) else {
            return Ok(None);
        };
        let mut stmt = self.conn.prepare(
            "SELECT id, title FROM nodes
              WHERE work_id = ?1 AND parent_id IS ?2 AND node_kind = ?3 AND deleted_at IS NULL
              ORDER BY sort_order, id",
        )?;
        let rows = stmt.query_map(params![work_id, parent, kind.as_str()], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut index = 0usize;
        let mut placed = 0usize;
        let mut any_numbered = false;
        for row in rows {
            let (id, sibling) = row?;
            if Some(id) == ignore {
                continue;
            }
            placed += 1;
            match parse_serial(&sibling, kind) {
                Some(other) => {
                    any_numbered = true;
                    if other < serial {
                        index = placed;
                    }
                }
                None => {}
            }
        }
        // **这一层压根没有编号**（散文 / 文集里作者都用自起的名字）→ 无号可归位：
        // 交给调用方"点哪儿插哪儿"。不这么拦一下的话，新章会落到整层**最上面**——
        // 因为"没有兄弟的号比它小"算出来的下标就是 0（文集里点「+」会插到第一篇之前）。
        if !any_numbered {
            return Ok(None);
        }
        Ok(Some(index))
    }

    pub fn add_chapter_after(&mut self, node_id: i64, kind: NodeKind, title: &str) -> Result<i64> {
        let parent: Option<i64> = self
            .conn
            .query_row(
                "SELECT parent_id FROM nodes WHERE id = ?1 AND deleted_at IS NULL",
                params![node_id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| {
                Error::invalid_with(codes::NODE_GONE, [("node_id", node_id.to_string())])
            })?;
        let work_id = self.node_work(node_id)?;
        // "点哪儿插哪儿"的落点：作者给了标题时听他的（序章 / 番外 / 补一章都用得上）
        let clicked = sibling_ids(&self.conn, work_id, parent)?
            .iter()
            .position(|id| *id == node_id)
            .map(|position| position + 1)
            .unwrap_or(usize::MAX);

        // 标题留给核心取号（顺序号）；要不要补上删掉的那一章由界面的弹窗问，不在这儿猜
        let auto = title.trim().is_empty();
        let created = self.create_node(work_id, parent, kind, title)?;

        // 落在哪儿——**两条规矩，按"号是谁定的"分**：
        //
        // - **标题留空（号是核心按"同层已用过的最大号 + 1"取的）→ 按号归位**：新号比同层
        //   所有号都大，于是它落在"最后一个有编号的章"之后，列表永远按号递增。
        //   真机报过：同层是 18/19/20/21，点「+」在第20章上建章，号取到 22、位置却插在 20 后面，
        //   屏幕上就成了 20 / 22 / 23 / 21——号与位置各说各话，看着就是"排序乱了"。
        // - **作者给了标题 → 点哪儿插哪儿**：名字是他自己定的（序章 / 楔子 / 番外 / 第X章·补），
        //   落点就该听他的，核心不替他改主意。
        let index = if auto {
            let created_title = self.node_title(created)?;
            self.index_by_serial(&created_title, work_id, parent, kind, Some(created))?
                .unwrap_or(clicked)
        } else {
            clicked
        };
        self.move_node(created, parent, index)?;
        Ok(created)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 认编号：只管开头、后缀可带尾巴；认不出的名字一概不算。
    #[test]
    fn parse_serial_reads_the_number_at_the_head() {
        let cases: &[(&str, Option<i64>)] = &[
            ("第12章", Some(12)),
            ("第12章 灯", Some(12)),
            ("第12章灯", Some(12)),
            ("第 12 章", Some(12)),
            ("第十二章", Some(12)),
            ("第十二章（上）", Some(12)),
            ("第壹章", Some(1)),
            ("第兩章", Some(2)),
            ("第貳章", Some(2)),
            ("第七十八章", Some(78)),
            ("第一百二十章", Some(120)),
            ("第两千零五章", Some(2005)),
            ("第壹佰贰拾章", Some(120)),
            ("第１０章", Some(10)),
            ("十十章", None),
            ("一百百章", None),
            ("第二卷 夜行", Some(2)),
            ("序章", None),
            ("楔子", None),
            ("第一次见面", None),
            ("第1-2章", None),
            ("场景卡", None),
            ("场景卡3", Some(3)),
            ("场景卡牌", None),
        ];
        for (title, want) in cases {
            let kind = if title.contains("卷") { NodeKind::Volume } else { NodeKind::Chapter };
            let kind = if title.starts_with("场景卡") { NodeKind::Scene } else { kind };
            assert_eq!(parse_serial(title, kind), *want, "标题「{title}」");
        }
    }

}
