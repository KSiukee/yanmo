//! 叩问命令域：**只问不写**——问题卡、选题、处置四件套与记灵感。
//!
//! 分工：模板池、引力公式、偏好学习、延后条件全在核心；这里只做三件事——
//! 把界面递进来的参数转给核心、把结果原样回给界面、**把句子的位置留空**。
//! 最后一条是要紧的：**核心零文案**，模板句子由界面按字典渲染（`question.template.*`），
//! 界面渲染好之后再作为"作者要问的这句话"交给核心落卡。
//!
//! 面板的数据形状是**一次给全**（候选 + 冷却库 + 静音的来源 + 还在等的延后）：
//! 它们是同一屏上的同一件事，分几次问只会让同一屏里前后对不上。
//!
//! 两个口径（与状态机一致，界面上也照这么说）：
//!
//! - **点开一张卡才算"问出"**（那一刻才消耗新颖度、才进冷却）；只看列表不算已问；
//! - **记灵感与处置正交**：记完灵感回到原问题，问题的状态一个字节都不动。

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::model::QuestionState;
use yanmo_core::question::{DeferPreset, QuestionDraft};
use yanmo_core::store::{
    Answer, CooledCard, Deferral, Inspiration, RoundItem, SelectedQuestion, Store,
};

/// 面板一次要的全部数据。
#[derive(Debug, Serialize)]
pub struct QuestionBoardDto {
    /// 候选（已按引力排好；每一条都带**拆解**，界面能回答"为什么先问这个"）
    pub selected: Vec<SelectedQuestion>,
    /// 冷却库：舍弃过的卡（可捞回）
    pub cooled: Vec<CooledCard>,
    /// 已经静音的来源（可解除）
    pub muted_sources: Vec<String>,
    /// 已经静音的**类别**（模板键；界面按字典渲染成"这类别再问"，可解除）
    pub muted_classes: Vec<String>,
    /// 还在等条件的延后（"有 N 条在等你说的那个时候"）
    pub open_deferrals: Vec<Deferral>,
}

/// 界面渲染好的一条候选：**句子在这时才成形**（核心只认键与槽位）。
#[derive(Debug, Deserialize)]
pub struct QuestionOffer {
    /// 哪条模板产的问题（进了卡就跟着它一辈子，偏好学习靠它）
    pub template_key: String,
    /// 界面按字典渲染好的那句话（作者看到的就是它）
    pub body: String,
    /// 关联锚点（`chapter:12` 这种；生成候选时给的，去重与溯源靠它）
    #[serde(default)]
    pub anchors: Vec<String>,
    pub importance: f64,
}

fn board(store: &Store, work_id: i64) -> yanmo_core::Result<QuestionBoardDto> {
    board_at(store, work_id, None)
}

/// 面板本体：`prefer_node` 给了就是模式 A 的「这一章优先」（排序在核心那一处）。
fn board_at(
    store: &Store,
    work_id: i64,
    prefer_node: Option<i64>,
) -> yanmo_core::Result<QuestionBoardDto> {
    let selected = match prefer_node {
        Some(node) => store.select_questions_for_chapter(work_id, node, 5)?,
        None => store.select_questions(work_id, 5)?,
    };
    Ok(QuestionBoardDto {
        selected,
        cooled: store.cooled_questions(work_id)?,
        muted_sources: store.muted_sources()?,
        muted_classes: store.muted_templates()?,
        open_deferrals: store.open_deferrals(work_id)?,
    })
}

/// 候选草稿：模板键 + 槽位取值（**不含句子**，界面按字典渲染）。
#[tauri::command(rename_all = "snake_case")]
pub fn question_drafts(
    data: State<'_, AppData>,
    work_id: i64,
) -> Result<Vec<QuestionDraft>, ApiError> {
    crate::acceptance::note_command("question_drafts");
    data.with_store(|store| store.question_drafts(work_id))
}

/// 把界面渲染好的问题落成卡（同模板同锚点不会重复造），并回一次最新面板。
#[tauri::command(rename_all = "snake_case")]
pub fn question_sync(
    data: State<'_, AppData>,
    work_id: i64,
    offers: Vec<QuestionOffer>,
) -> Result<QuestionBoardDto, ApiError> {
    crate::acceptance::note_command("question_sync");
    data.with_store(|store| {
        for offer in offers {
            store.create_question_card(&yanmo_core::model::NewQuestionCard {
                work_id,
                body: offer.body,
                // 核心自带的问题：来源写 `core`（模块提交的问题由模块自己标来源）
                source: "core".to_string(),
                template_key: offer.template_key,
                importance: offer.importance,
                linked: offer.anchors,
                derived_from: None,
                auto_derived: false,
            })?;
        }
        board(store, work_id)
    })
}

/// 看一眼面板（不写库）。
///
/// 给了 `node_id` 就按**「这一章优先」**排候选（模式 A）：与这一章有关的问题排最前，
/// 其余照旧按引力跟着。排序规则在核心——界面只把当前章报上来，不在这边重排一遍。
#[tauri::command(rename_all = "snake_case")]
pub fn question_board(
    data: State<'_, AppData>,
    work_id: i64,
    node_id: Option<i64>,
) -> Result<QuestionBoardDto, ApiError> {
    data.with_store(|store| board_at(store, work_id, node_id))
}

/// 落章：把一条答案标成「落进过正文」——**只留痕，不写正文**。
///
/// 正文那一段字由界面插进编辑会话（于是自动落盘、字数、账本、版本快照全照常走）：
/// 击键级的正文写入必须留在壳内的编辑会话里，核心直接改正文会让两边分家。
/// 这里做的是另一半——记下"这一条用掉了、落到哪一章"。
#[tauri::command(rename_all = "snake_case")]
pub fn question_land_answer(
    data: State<'_, AppData>,
    card_id: i64,
    node_id: i64,
) -> Result<QuestionBoardDto, ApiError> {
    crate::acceptance::note_command("question_land_answer");
    data.with_store(|store| {
        let work_id = store.question_card(card_id)?.work_id;
        store.mark_answer_landed(card_id, node_id, "author")?;
        board(store, work_id)
    })
}

/// 问出这一张：状态从「待问」到「已问」——**新颖度从这一刻开始算**，与延后/作答同一条来路。
#[tauri::command(rename_all = "snake_case")]
pub fn question_ask(data: State<'_, AppData>, card_id: i64) -> Result<QuestionBoardDto, ApiError> {
    crate::acceptance::note_command("question_ask");
    data.with_store(|store| {
        let work_id = store.question_card(card_id)?.work_id;
        store.move_question_card(card_id, QuestionState::Asked, "pull")?;
        board(store, work_id)
    })
}

/// 作答的回执：**核心落下来的那一条**答案 + 落完之后的最新面板。
///
/// 回执为什么要从核心读回来（而不是把界面传进去的那份原样返回）：界面显示的就该是
/// **库里真有的东西**——修剪后的原文、核心认下的输入方式。自己回显自己等于没核对。
#[derive(Debug, Serialize)]
pub struct AnswerReceiptDto {
    pub answer: Answer,
    pub board: QuestionBoardDto,
}

/// 作答：把答案落成一张碎片（**不动正文**），并把问题卡走到「已答」终态。
///
/// `source` 是**怎么打出来的**（`typed` / `voice` / `mixed`）——它与答案文本解耦，
/// 是创作留痕的原始素材，所以照原样交给核心、由核心那一处校验（认不出就拒，界面只报码）。
/// 答案落到章里是两条落点模式（穿插式 / 先问后排版）的事，本命令一个字节都不写正文。
#[tauri::command(rename_all = "snake_case")]
pub fn question_answer(
    data: State<'_, AppData>,
    card_id: i64,
    body: String,
    source: String,
) -> Result<AnswerReceiptDto, ApiError> {
    crate::acceptance::note_command("question_answer");
    data.with_store(|store| {
        let work_id = store.question_card(card_id)?.work_id;
        store.record_question_answer(card_id, &body, &source, "author")?;
        Ok(AnswerReceiptDto { answer: store.answer_of_question(card_id)?, board: board(store, work_id)? })
    })
}

/// 一轮落章（先问后排版）：把这一轮攒下的答案**一次**落进这一章。
///
/// 与 [`question_land_answer`] 的分工：那条是"答一条落一条"，这条是"一轮问完一次落"。
/// 正文那几段字由界面**一次**插进编辑会话（同一条编辑路，自动落盘照常）；这里做的是账——
/// 回写作者改过的字、把每条标成落过、逐条留痕，一个事务。
#[tauri::command(rename_all = "snake_case")]
pub fn question_apply_round(
    data: State<'_, AppData>,
    work_id: i64,
    node_id: i64,
    items: Vec<RoundItem>,
) -> Result<QuestionBoardDto, ApiError> {
    crate::acceptance::note_command("question_apply_round");
    data.with_store(|store| {
        store.apply_answer_round(work_id, node_id, &items, "author")?;
        board(store, work_id)
    })
}

/// 延后：按界面那一档预置（一天/三天/一周/写完这一章/我自己想起来），可带作者填的一句。
#[tauri::command(rename_all = "snake_case")]
pub fn question_defer(
    data: State<'_, AppData>,
    card_id: i64,
    preset: String,
    note: String,
) -> Result<QuestionBoardDto, ApiError> {
    crate::acceptance::note_command("question_defer");
    let preset = DeferPreset::parse(&preset)
        .ok_or_else(|| ApiError::with("shell.question_bad_preset", [("value", preset.clone())]))?;
    data.with_store(|store| {
        let work_id = store.question_card(card_id)?.work_id;
        store.defer_question_card_by_preset(card_id, preset, &note, "author")?;
        board(store, work_id)
    })
}

/// 舍弃：进冷却库（可捞回，同时作为选题的负样本）。
#[tauri::command(rename_all = "snake_case")]
pub fn question_discard(
    data: State<'_, AppData>,
    card_id: i64,
) -> Result<QuestionBoardDto, ApiError> {
    crate::acceptance::note_command("question_discard");
    data.with_store(|store| {
        let work_id = store.question_card(card_id)?.work_id;
        store.move_question_card(card_id, QuestionState::Discarded, "author")?;
        board(store, work_id)
    })
}

/// 说「这个问题好」：状态不动，只教同类模板（评价与处置是两件事）。
#[tauri::command(rename_all = "snake_case")]
pub fn question_praise(
    data: State<'_, AppData>,
    card_id: i64,
) -> Result<QuestionBoardDto, ApiError> {
    crate::acceptance::note_command("question_praise");
    data.with_store(|store| {
        let work_id = store.question_card(card_id)?.work_id;
        store.praise_question_card(card_id, "author")?;
        board(store, work_id)
    })
}

/// 永久静音**这一类**：这一类以后都别问了（模板权重里置为停用，可撤销）。
#[tauri::command(rename_all = "snake_case")]
pub fn question_mute_class(
    data: State<'_, AppData>,
    card_id: i64,
) -> Result<QuestionBoardDto, ApiError> {
    crate::acceptance::note_command("question_mute_class");
    data.with_store(|store| {
        let work_id = store.question_card(card_id)?.work_id;
        store.move_question_card(card_id, QuestionState::Muted, "author")?;
        board(store, work_id)
    })
}

/// 解除**这一类**的静音：面板上"这类别再问"的回头路（少了它就是个只进不出的开关）。
#[tauri::command(rename_all = "snake_case")]
pub fn question_unmute_class(
    data: State<'_, AppData>,
    work_id: i64,
    template_key: String,
) -> Result<QuestionBoardDto, ApiError> {
    crate::acceptance::note_command("question_unmute_class");
    data.with_store(|store| {
        store.unmute_template(&template_key)?;
        board(store, work_id)
    })
}

/// **别等了**：取消延后、当场回候选池（"我自己想起来再问"那条的回头路）。
#[tauri::command(rename_all = "snake_case")]
pub fn question_undefer(
    data: State<'_, AppData>,
    card_id: i64,
) -> Result<QuestionBoardDto, ApiError> {
    crate::acceptance::note_command("question_undefer");
    data.with_store(|store| {
        let work_id = store.question_card(card_id)?.work_id;
        store.cancel_deferral(card_id, "author")?;
        board(store, work_id)
    })
}

/// 从冷却库捞回（舍弃不真删）。
#[tauri::command(rename_all = "snake_case")]
pub fn question_retrieve(
    data: State<'_, AppData>,
    card_id: i64,
) -> Result<QuestionBoardDto, ApiError> {
    crate::acceptance::note_command("question_retrieve");
    data.with_store(|store| {
        let work_id = store.question_card(card_id)?.work_id;
        store.move_question_card(card_id, QuestionState::Pending, "author")?;
        board(store, work_id)
    })
}

/// 记灵感：**不动问题状态**，只落一张带溯源的灵感卡（界面记完回到原问题）。
#[tauri::command(rename_all = "snake_case")]
pub fn question_inspire(
    data: State<'_, AppData>,
    card_id: i64,
    body: String,
    source: String,
) -> Result<Inspiration, ApiError> {
    crate::acceptance::note_command("question_inspire");
    data.with_store(|store| {
        let idea_id = store.record_question_inspiration(card_id, &body, &source, "author")?;
        store.inspiration(idea_id)
    })
}

/// 按来源静音：某个模块太吵时**只让它闭嘴**，不是把整个叩问关掉。
#[tauri::command(rename_all = "snake_case")]
pub fn question_mute_source(
    data: State<'_, AppData>,
    work_id: i64,
    source: String,
) -> Result<QuestionBoardDto, ApiError> {
    crate::acceptance::note_command("question_mute_source");
    data.with_store(|store| {
        store.mute_source(&source)?;
        board(store, work_id)
    })
}

/// 解除某个来源的静音。
#[tauri::command(rename_all = "snake_case")]
pub fn question_unmute_source(
    data: State<'_, AppData>,
    work_id: i64,
    source: String,
) -> Result<QuestionBoardDto, ApiError> {
    crate::acceptance::note_command("question_unmute_source");
    data.with_store(|store| {
        store.unmute_source(&source)?;
        board(store, work_id)
    })
}

/// 把**条件已经满足**的延后放回候选池，并回一次最新面板。
///
/// 什么时候喊这一声由界面定（打开面板 / 每几分钟 / 建完一章）——它是低频动作，
/// 绝不能挂在击键那条路上。
#[tauri::command(rename_all = "snake_case")]
pub fn question_requeue_due(
    data: State<'_, AppData>,
    work_id: i64,
) -> Result<QuestionBoardDto, ApiError> {
    crate::acceptance::note_command("question_requeue_due");
    data.with_store(|store| {
        store.requeue_due_questions(work_id, yanmo_core::time::now_millis(), "shell")?;
        board(store, work_id)
    })
}
