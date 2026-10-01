//! 候选池：**该问什么**——从书里已有的东西生成草稿，并滤掉已经问过的。
//!
//! 三件事都在这儿，因为它们共用同一份"书此刻长什么样"：
//!
//! 1. [读要素](Store::writing_elements)：五路事实——按阅读顺序的节点（承载正文的）、
//!    伏笔那本账（埋点换算成阅读顺序里的下标）、碎片池里引力最高的前几条、
//!    人物那本账（人物卡与出场节点现算出来的"静了多少章"）、
//!    五线标注（每条线最后一次被推进之后隔了多少章）；
//! 2. [生草稿](Store::question_drafts)：按规则产出模板 + 槽位，**滤掉已经问过的**；
//! 3. 派生关系的守门（[`validate_derivation`]）：来源必须同书、自动派生不许无限套娃。
//!
//! 草稿里没有一个字的句子——句子由界面按语言渲染（核心零文案），作者点头才落成卡。

use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension};

use super::Store;
use crate::error::{codes, Error, Result};
use crate::gravity::allowed_auto_derivation;
use crate::model::{EntityKind, StoryLine};
use crate::question::{
    generate, ChapterFacts, CharacterFacts, ForeshadowFacts, LineFacts, QuestionDraft,
    RhythmParams, SparkFacts, WritingElements,
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
    /// 读一本书的写作要素：五路事实各读各的。
    ///
    /// 1. **章节**：按阅读顺序、承载正文的那些节点（容器节点滤掉）；
    /// 2. **伏笔**：埋点换算成**阅读顺序里的下标**（判"隔了多少章"靠它；埋点不在
    ///    这份章节清单里的——删了、指向别处——就是 `None`，规则会跳过，绝不猜）；
    /// 3. **灵感碎片**：作者随手记的那几种里**引力最高的前 [`SPARK_ELEMENTS`] 条**
    ///    （引力用现成的 [`Store::spark_candidates`]——同一套公式、同一个排序，不另算一份）；
    /// 4. **人物**：人物卡 × 出场账（`node_cast`）→ 每个人**最后一次出场之后**隔了多少章
    ///    （与伏笔**同一份坐标**；从没出场过、或出场节点不在那份清单里的就是 `None`，
    ///    规则跳过，绝不猜——这是"人物线"那条**派生推进度**的读数口）；
    /// 5. **五线**：五线标注（`node_lines`）→ 每条线**最后一次被推进之后**隔了多少章
    ///    （**同一份坐标**；一次都没标过的就是 `None`——"没标"不等于"没推进"，
    ///    规则跳过，绝不猜。这是作者自己标出来的那一路推进度的读数口）。
    ///
    /// 焦点给 `None`：这一层答的是"这本书此刻长什么样"，而"作者正打开着哪一章"是
    /// 每次**选题**时才知道的事（见 [`Store::select_questions_for_chapter`]）——
    /// 于是关联度一律 1.0，谁上场跟作者此刻在哪一章无关。
    pub fn writing_elements(&self, work_id: i64) -> Result<WritingElements> {
        let chapters = self.chapter_facts(work_id)?;

        // 伏笔与人物判"隔了多少章 / 静了多少章"：**按"正文单位"那一份清单算**
        // （`text_spine`：承载正文、不是场景卡）——与大纲体检那条同阈值判据同一份坐标。
        // 上面 `chapters` 那份含场景卡（章节类规则要用它），两份清单各司其职，
        // 别拿一份去数另一件事的章数。**一处算、两条线共用一个数**（见 `spine_index`）。
        let (spine_last, spine_pos) = self.spine_index(work_id)?;
        let foreshadows = self
            .foreshadows(work_id, None)?
            .into_iter()
            .map(|item| ForeshadowFacts {
                id: item.id,
                body: item.body,
                state: item.state,
                chapters_since: item
                    .planted_node
                    .and_then(|node| spine_pos.get(&node).copied())
                    .map(|at| spine_last - at),
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

        // 人物线：人物卡 × 出场账 → "静了多少章"（与伏笔**同一份** `spine_index` 坐标）
        // 人物线：人物卡 × 出场账 → "静了多少章"（与伏笔**同一份** `spine_index` 坐标）
        let characters = self.character_facts(work_id, spine_last, &spine_pos)?;
        // 五线：作者标过的推进度 → 每条线"多久没被推过"（**同一份**坐标）
        let lines = self.line_facts(work_id, spine_last, &spine_pos)?;

        Ok(WritingElements { chapters, foreshadows, sparks, characters, lines })
    }

    /// 这本书的**章节事实**：按阅读顺序、承载正文的节点（容器节点滤掉）。
    ///
    /// 两处共用同一份样本：叩问那五路事实里的章节，与**章长判据**的分布
    /// （见 `super::chapter_length`）。样本口径要是各筛各的，中位数就会悄悄不一样——
    /// 而"这本书中位多少字"是要念给作者听的事实（见 `crate::question::rhythm`）。
    pub(super) fn chapter_facts(&self, work_id: i64) -> Result<Vec<ChapterFacts>> {
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
        Ok(chapters)
    }

    /// **正文单位**（`text_spine`）在阅读顺序里的下标：返回（末章下标, 节点 → 下标）。
    ///
    /// 为什么抽出来一处算：伏笔超期与人物静默数的是**同一个东西**（"从某个节点到末章隔了几章"）。
    /// 各写一套的话，哪天要换清单就得记得换两处——漏一处，两条线立刻各说各话
    /// （0.92.1 修的就是这种"两份清单打架"的账）。数在哪算、就按哪的口径。
    fn spine_index(&self, work_id: i64) -> Result<(i64, HashMap<i64, i64>)> {
        let spine = self.text_spine(work_id)?;
        let last = spine.len().saturating_sub(1) as i64;
        let pos = spine
            .into_iter()
            .enumerate()
            .map(|(at, node)| (node.id, at as i64))
            .collect();
        Ok((last, pos))
    }

    /// 一本书的**人物线事实**：每个还活着的人物卡静了多久（**现算，不存**——这条派生线
    /// 不新建任何数据模型）。
    ///
    /// 三条口径：
    ///
    /// 1. **只认人物卡**（[`EntityKind::Person`]）：出场那一格的选人卡只摆人物
    ///    （见 `app/frontend/src/components/grid-cast.ts`），地点 / 物品挂上节点也叫"出场"，
    ///    但它们不"退场"——这条线说的是人；
    /// 2. **出场账只数正文单位里的节点**（`spine_pos`）：人物挂过的节点若不在那份清单里
    ///    （场景卡、已删的、别的书——`node_cast` 已按书过滤），那一次出场不算数；
    /// 3. **判不了的不猜**：从没出场过的（出场账里根本没有他）与上面那条不成立的，
    ///    一律 `None`；名字空着的**直接跳过**（界面不替作者编名字，也不拿空格去填句子）。
    ///
    /// 顺序＝ [`Store::entity_cards`] 给的顺序（按名字排、id 收尾），两次调用结果一样。
    fn character_facts(
        &self,
        work_id: i64,
        spine_last: i64,
        spine_pos: &HashMap<i64, i64>,
    ) -> Result<Vec<CharacterFacts>> {
        // 出场账：人物 id → **最后一次**出场（正文单位里的下标）。一个人挂过好几章，
        // 取最靠后的那一次（"最后一次露面"）。
        let mut last_at: HashMap<i64, i64> = HashMap::new();
        for (node, members) in self.node_cast(work_id)? {
            let Some(at) = spine_pos.get(&node).copied() else { continue };
            for member in members {
                let seen = last_at.entry(member.entity_id).or_insert(at);
                if at > *seen {
                    *seen = at;
                }
            }
        }
        let mut out = Vec::new();
        for card in self.entity_cards(work_id, Some(EntityKind::Person))? {
            if card.name.trim().is_empty() {
                continue;
            }
            out.push(CharacterFacts {
                id: card.id,
                name: card.name,
                chapters_since: last_at.get(&card.id).map(|at| spine_last - at),
            });
        }
        Ok(out)
    }

    /// 一本书的**五线事实**：每条线**最后一次被推进之后**隔了多少章（**现算，不存**——
    /// 这条推进度读的是作者自己标的 `node_lines`，不新建任何数据模型）。
    ///
    /// 三条口径：
    ///
    /// 1. **只数正文单位里的节点**（`spine_pos`）：标注本来就只有正文单位能落（
    ///    `set_node_lines` 当场拒卷与场景卡），这里再兜一道——那份清单外的节点（场景卡、
    ///    已删的、别的书——`node_lines` 已按书过滤）就算有几行脏标注也不算数；
    /// 2. **一次都没标过的照样进清单，只是 `None`**：五条线各有一条事实，顺序＝
    ///    [`StoryLine::ALL`]（与人物那本账同一个写法：清单的形状不随作者标没标而变）。
    ///    为什么不是"没标过的就不进清单"：`None` 在这个结构里**有确切含义**（"没说过"），
    ///    而"少一条"只能靠调用方自己发现——那正是最难查的一类静默（见 `LineFacts`）；
    /// 3. **判不了的不猜**：`None` 的线规则一律跳过——"没标"不是"没推进"
    ///    （这是 [`StoryLine`] 写死的口径），绝不把"还没说过"说成"断了"。
    ///
    /// 顺序＝ [`StoryLine::ALL`]（五条线的固定序），两次调用结果一样。
    fn line_facts(
        &self,
        work_id: i64,
        spine_last: i64,
        spine_pos: &HashMap<i64, i64>,
    ) -> Result<Vec<LineFacts>> {
        // 标注账：线 → **最后一次**被推进（正文单位里的下标）。一条线可能在好几章上标过，
        // 取最靠后的那一次（"最后一次被推"）。
        let mut last_at: HashMap<StoryLine, i64> = HashMap::new();
        for (node, lines) in self.node_lines(work_id)? {
            let Some(at) = spine_pos.get(&node).copied() else { continue };
            for line in lines {
                let seen = last_at.entry(line).or_insert(at);
                if at > *seen {
                    *seen = at;
                }
            }
        }
        Ok(StoryLine::ALL
            .into_iter()
            .map(|line| LineFacts {
                line,
                chapters_since: last_at.get(&line).map(|at| spine_last - at),
            })
            .collect())
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
