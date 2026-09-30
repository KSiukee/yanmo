//! 候选池：**该问什么**——从书里已有的东西生成草稿，并滤掉已经问过的。
//!
//! 三件事都在这儿，因为它们共用同一份"书此刻长什么样"：
//!
//! 1. [读要素](Store::writing_elements)：三路事实——按阅读顺序的节点（承载正文的）、
//!    伏笔那本账（埋点换算成阅读顺序里的下标）、碎片池里引力最高的前几条；
//! 2. [生草稿](Store::question_drafts)：按规则产出模板 + 槽位，**滤掉已经问过的**；
//! 3. 派生关系的守门（[`validate_derivation`]）：来源必须同书、自动派生不许无限套娃。
//!
//! 草稿里没有一个字的句子——句子由界面按语言渲染（核心零文案），作者点头才落成卡。

use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension};

use super::Store;
use crate::error::{codes, Error, Result};
use crate::gravity::allowed_auto_derivation;
use crate::question::{
    generate, ChapterFacts, ForeshadowFacts, QuestionDraft, RhythmParams, SparkFacts,
    WritingElements,
};

/// 生成叩问候选时，取**引力最高的前几条**灵感碎片（旋钮，3~5 之间都好用）。
///
/// 3 的理由：碎片这条线一屏只摆一条（`store::question_select` 的"同类不扎堆"），
/// 多出来的草稿只是在队列里排着；而每次刷新都会把新草稿**落成卡**，
/// 取太多会让卡池悄悄涨起来（作者没答过的问题堆一库）。取 3 条：
/// 头一条最该问，另外两条是"它已经问过 / 被过滤"时的后备，再多的边际价值很小。
pub const SPARK_ELEMENTS: usize = 3;

/// 派生链最多往回追这么多跳——环与手改出来的深链都不许让它转到天荒地老。
const MAX_DERIVATION_WALK: usize = 16;

/// 一张碎片往回追到根有几跳（不是派生出来的 = 0）。
///
/// ⚠️ 只沿 `derived_from` 走、**与碎片的种类无关**：链条本来就会穿过灵感卡
/// （问题 → 作者记的灵感 → 由这条灵感自动派生的问题）。要是只跟"问题卡"这一类走，
/// 链走到灵感卡就断了，防自激的链深限制形同虚设——这是 2026-09-15 落地"记灵感"时
/// 当场发现的真 bug（当时的实现只看 `frag_kind = 'question'` 的父级）。
pub(super) fn derivation_depth(conn: &Connection, card_id: i64) -> Result<usize> {
    let mut depth = 0usize;
    let mut cursor = Some(card_id);
    while let Some(id) = cursor {
        if depth >= MAX_DERIVATION_WALK {
            break;
        }
        depth += 1;
        let next: Option<Option<i64>> = conn
            .query_row(
                "SELECT derived_from FROM fragments WHERE id = ?1 AND deleted_at IS NULL",
                params![id],
                |r| r.get(0),
            )
            .optional()?;
        cursor = next.flatten();
    }
    Ok(depth.saturating_sub(1))
}

/// 派生关系成立吗：
///
/// - **自动派生的必须说清来源**（没来源的"自动"是说不通的）；
/// - 来源必须是**同一本书**里的一张碎片（问题卡或作者记下的灵感卡都行；跨书溯源
///   等于把两本书的因果混在一起）；
/// - **自动派生**不许越过链深上限——作者手动基于灵感再问，不受此限
///   （深度限制是为了防问题池自我膨胀，不是为了拦作者）。
pub(super) fn validate_derivation(
    conn: &Connection,
    work_id: i64,
    derived_from: Option<i64>,
    auto_derived: bool,
    max_depth: usize,
) -> Result<()> {
    if auto_derived && derived_from.is_none() {
        return Err(Error::invalid(codes::CARD_AUTO_DERIVED_NEEDS_SOURCE));
    }
    let Some(source) = derived_from else { return Ok(()) };
    let owner: Option<i64> = conn
        .query_row(
            "SELECT work_id FROM fragments WHERE id = ?1 AND deleted_at IS NULL",
            params![source],
            |r| r.get(0),
        )
        .optional()?;
    if owner != Some(work_id) {
        return Err(Error::invalid_with(
            codes::CARD_DERIVED_FROM_INVALID,
            [("derived_from", source.to_string()), ("work_id", work_id.to_string())],
        ));
    }
    if auto_derived && !allowed_auto_derivation(derivation_depth(conn, source)?, max_depth) {
        return Err(Error::invalid_with(
            codes::CARD_DERIVATION_TOO_DEEP,
            [("max", max_depth.to_string())],
        ));
    }
    Ok(())
}

impl Store {
    /// 读一本书的写作要素：三路事实各读各的。
    ///
    /// 1. **章节**：按阅读顺序、承载正文的那些节点（容器节点滤掉）；
    /// 2. **伏笔**：埋点换算成**阅读顺序里的下标**（判"隔了多少章"靠它；埋点不在
    ///    这份章节清单里的——删了、指向别处——就是 `None`，规则会跳过，绝不猜）；
    /// 3. **灵感碎片**：作者随手记的那几种里**引力最高的前 [`SPARK_ELEMENTS`] 条**
    ///    （引力用现成的 [`Store::spark_candidates`]——同一套公式、同一个排序，不另算一份）。
    ///
    /// 焦点给 `None`：这一层答的是"这本书此刻长什么样"，而"作者正打开着哪一章"是
    /// 每次**选题**时才知道的事（见 [`Store::select_questions_for_chapter`]）——
    /// 于是关联度一律 1.0，谁上场跟作者此刻在哪一章无关。
    pub fn writing_elements(&self, work_id: i64) -> Result<WritingElements> {
        let mut chapters = Vec::new();
        for node in self.list_nodes(work_id)? {
            if !node.kind.holds_body() {
                continue;
            }
            chapters.push(ChapterFacts {
                node_id: node.id,
                // 用**渲染后**的标题问问题（"第3章"而不是存着的模板 `第{$N}章`）
                title: node.title_rendered,
                has_body: node.has_body,
                char_count: node.char_count,
                summary: node.summary,
            });
        }

        // 埋点 → 下标：拿刚读出来的这份清单当坐标（规则算"隔了多少章"用的也是它，
        // 两处同一个坐标系，不会一个按章数、一个按节点数）
        let index_of: HashMap<i64, usize> =
            chapters.iter().enumerate().map(|(at, chapter)| (chapter.node_id, at)).collect();
        let foreshadows = self
            .foreshadows(work_id, None)?
            .into_iter()
            .map(|item| ForeshadowFacts {
                id: item.id,
                body: item.body,
                state: item.state,
                planted_index: item.planted_node.and_then(|node| index_of.get(&node).copied()),
            })
            .collect();

        let sparks = self
            .spark_candidates(work_id, None, SPARK_ELEMENTS)?
            .into_iter()
            .map(|spark| SparkFacts {
                fragment_id: spark.id,
                body: spark.body,
                total: spark.gravity.total,
                anchors: spark.anchors,
            })
            .collect();

        Ok(WritingElements { chapters, foreshadows, sparks })
    }

    /// 一本书此刻的候选草稿：按规则生成，**滤掉已经问过的**（同模板 + 同锚点）。
    ///
    /// "已经问过"= 库里有一张同模板、**这些锚点全在**的卡——**任何状态都算**（含舍弃与静音）：
    /// 冷却库里的卡可以捞回，但不该在下次生成时又冒出一张重复的。
    ///
    /// ⚠️ 判据是"**全部锚点都问过**"而不是"任意一个锚点问过"：一条草稿可能带两个锚点
    /// （灵感碎片＝`fragment:<id>` + 它挂的 `chapter:<id>`）。按"任意一个"算的话，
    /// 挂在同一章上的第二条灵感会被第一条的章节锚点连坐——**同一条碎片问过没有**才是身份，
    /// 章只是它顺便带的落点。（单锚点的草稿两种写法等价，老规矩没变。）
    pub fn question_drafts(&self, work_id: i64) -> Result<Vec<QuestionDraft>> {
        let elements = self.writing_elements(work_id)?;
        let drafts = generate(&elements, &RhythmParams::default());
        let known = self.card_anchors(work_id)?;
        Ok(drafts
            .into_iter()
            .filter(|draft| {
                // 保留条件是"**还没问过**"：至少有一个锚点没在库里出现过
                !draft.anchors.is_empty()
                    && draft.anchors.iter().any(|anchor| {
                        !known.contains(&(draft.template_key.to_string(), anchor.clone()))
                    })
            })
            .collect())
    }

    /// 书里已有的（模板键, 锚点）对——生成时靠它去重。
    fn card_anchors(&self, work_id: i64) -> Result<HashSet<(String, String)>> {
        let mut out = HashSet::new();
        for card in self.question_cards(work_id, None)? {
            // 锚点存在 `linked` 里（JSON 数组文本）；读不动的当"没有锚点"——
            // 最坏的结果是去重少认一条，不是少问一条（宁可多问，不可消失）
            let anchors: Vec<String> = serde_json::from_str(&card.linked).unwrap_or_default();
            for anchor in anchors {
                out.insert((card.template_key.clone(), anchor));
            }
        }
        Ok(out)
    }
}
