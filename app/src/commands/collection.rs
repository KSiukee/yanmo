//! 短篇集命令：一屏读数、加 / 拿掉 / 重排、定目标、一键建集。
//!
//! 规则全在核心（`store::collection` + `store::collection_row`）：这里只做参数转换 +
//! 把读出来给人用的那份递回去。**集只是引用**——加、拿掉、重排都不碰正文，
//! 所以这一族命令里没有"写正文"这种口子。

use serde::Serialize;
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::store::{ArticleEntry, CollectionView, Store};

/// 集里的一篇（"还没归集的散篇"用的是同一张形状）。
#[derive(Debug, Serialize)]
pub struct ArticleEntryDto {
    pub id: i64,
    pub title: String,
    /// 这一篇的流转状态（七态稳定码）
    pub status: String,
    pub word_count: i64,
    pub char_count: i64,
    pub chars_no_punct: i64,
    pub updated_at: i64,
}

impl From<ArticleEntry> for ArticleEntryDto {
    fn from(entry: ArticleEntry) -> Self {
        Self {
            id: entry.id,
            title: entry.title,
            status: entry.status.as_str().to_string(),
            word_count: entry.word_count,
            char_count: entry.char_count,
            chars_no_punct: entry.chars_no_punct,
            updated_at: entry.updated_at,
        }
    }
}

/// 一个集的一屏：篇目（按集内顺序）+ 目标 + 现有规模。
#[derive(Debug, Serialize)]
pub struct CollectionViewDto {
    pub id: i64,
    pub title: String,
    pub entries: Vec<ArticleEntryDto>,
    /// 目标篇数（null = 没设过）
    pub target_pieces: Option<i64>,
    /// 目标总字数（null = 没设过）
    pub target_words: Option<i64>,
    /// 现有篇数与字数合计（预聚合口径，不扫正文）
    pub pieces: i64,
    pub words: i64,
    pub chars: i64,
    pub chars_no_punct: i64,
}

fn to_dto(view: CollectionView) -> CollectionViewDto {
    // 汇总先在移动 `entries` 之前算好（`pieces()` 那几个方法读的是它）
    let (pieces, words, chars, chars_no_punct) =
        (view.pieces(), view.words(), view.chars(), view.chars_no_punct());
    CollectionViewDto {
        id: view.collection.id,
        title: view.collection.title,
        entries: view.entries.into_iter().map(ArticleEntryDto::from).collect(),
        target_pieces: view.target_pieces,
        target_words: view.target_words,
        pieces,
        words,
        chars,
        chars_no_punct,
    }
}

/// 一个集的一屏（篇目按集内顺序）。
#[tauri::command(rename_all = "snake_case")]
pub fn collection_view(data: State<'_, AppData>, work_id: i64) -> Result<CollectionViewDto, ApiError> {
    data.with_store(|store: &mut Store| Ok(to_dto(store.collection_view(work_id)?)))
}

/// 往集里加一篇（**追加在末尾**；已经在里面就是空操作）；回读那一屏。
#[tauri::command(rename_all = "snake_case")]
pub fn collection_add(
    data: State<'_, AppData>,
    work_id: i64,
    article_id: i64,
) -> Result<CollectionViewDto, ApiError> {
    data.with_store(|store: &mut Store| {
        store.add_to_collection(work_id, article_id)?;
        Ok(to_dto(store.collection_view(work_id)?))
    })
}

/// 从集里拿掉一篇（**只去引用，不碰正文**）；回读那一屏。
#[tauri::command(rename_all = "snake_case")]
pub fn collection_remove(
    data: State<'_, AppData>,
    work_id: i64,
    article_id: i64,
) -> Result<CollectionViewDto, ApiError> {
    data.with_store(|store: &mut Store| {
        store.remove_from_collection(work_id, article_id)?;
        Ok(to_dto(store.collection_view(work_id)?))
    })
}

/// 按给定顺序重排集内篇目（名单与集里对不上会被核心拒绝）；回读那一屏。
#[tauri::command(rename_all = "snake_case")]
pub fn collection_reorder(
    data: State<'_, AppData>,
    work_id: i64,
    article_ids: Vec<i64>,
) -> Result<CollectionViewDto, ApiError> {
    data.with_store(|store: &mut Store| {
        store.reorder_collection(work_id, &article_ids)?;
        Ok(to_dto(store.collection_view(work_id)?))
    })
}

/// 定集目标（篇数 / 总字数；null 或 ≤0 = 清掉）；回读那一屏。
#[tauri::command(rename_all = "snake_case")]
pub fn collection_set_target(
    data: State<'_, AppData>,
    work_id: i64,
    pieces: Option<i64>,
    words: Option<i64>,
) -> Result<CollectionViewDto, ApiError> {
    data.with_store(|store: &mut Store| {
        store.set_collection_target(work_id, pieces, words)?;
        Ok(to_dto(store.collection_view(work_id)?))
    })
}

/// 一键建集：建一个集，把这几篇按给定顺序装进去；回读新集的这一屏。
#[tauri::command(rename_all = "snake_case")]
pub fn collection_create(
    data: State<'_, AppData>,
    title: String,
    article_ids: Vec<i64>,
) -> Result<CollectionViewDto, ApiError> {
    data.with_store(|store: &mut Store| {
        let work = store.create_collection_from(&title, &article_ids)?;
        Ok(to_dto(store.collection_view(work.id)?))
    })
}

/// **还没归集的散篇**（一篇都不在任何活着的集里）：攒集提示与"往集里加一篇"那张清单都用它。
#[tauri::command]
pub fn collection_loose_articles(data: State<'_, AppData>) -> Result<Vec<ArticleEntryDto>, ApiError> {
    data.with_store(|store: &mut Store| {
        Ok(store.uncollected_articles()?.into_iter().map(ArticleEntryDto::from).collect())
    })
}

/// 这一篇属于哪几个（活着的）集——稿件台的详情用它说"已收进哪几本集子"。
#[tauri::command(rename_all = "snake_case")]
pub fn collection_of(data: State<'_, AppData>, article_id: i64) -> Result<Vec<i64>, ApiError> {
    data.with_store(|store: &mut Store| store.collections_of(article_id))
}
