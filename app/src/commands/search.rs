//! 检索：在正文里找一句话。
//!
//! 与别的域一样，命令只做"参数转换 + 转交核心"：**检索语义全在核心那一份实现里**
//! （`store::search`：≥3 字走 FTS5 trigram 索引、1–2 字回退全扫、片段怎么取、
//! 已删除的章节与作品一律不出现在结果里）。
//!
//! 这里只补一样核心给不了的东西：**书名**。结果里写「哪本书的哪一章」比一个 work_id 好认得多，
//! 而核心的命中只带 work_id（它不认识"书架"这件事）。

use std::collections::HashMap;

use serde::Serialize;
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::store::{SearchHit, Store};

/// 一条命中。
#[derive(Debug, Serialize)]
pub struct SearchHitDto {
    pub node_id: i64,
    pub work_id: i64,
    /// 命中所在的书的书名；书还没起名时是空串（界面自己给占位，壳不替作者编名字）
    pub work_title: String,
    /// 章名 / 篇名
    pub title: String,
    /// 命中周围的片段；只在标题里命中时是空串
    pub snippet: String,
    /// 命中在标题里（界面可以据此把标题标出来）
    pub matched_title: bool,
}

/// 把核心的命中补成界面上能直接念的一条。
fn to_dto(hit: SearchHit, titles: &HashMap<i64, String>) -> SearchHitDto {
    SearchHitDto {
        node_id: hit.node_id,
        work_id: hit.work_id,
        work_title: titles.get(&hit.work_id).cloned().unwrap_or_default(),
        title: hit.title,
        snippet: hit.snippet,
        matched_title: hit.matched_title,
    }
}

/// 检索正文：`work_id` 传 `null` 就是**所有书**，传一个 id 就是只看那一本。
///
/// `limit` 给 0 表示用核心的默认上限（50）；核心另有自己的硬上限（500），这里不再抄一遍。
#[tauri::command(rename_all = "snake_case")]
pub fn search_text(
    data: State<'_, AppData>,
    query: String,
    work_id: Option<i64>,
    limit: usize,
) -> Result<Vec<SearchHitDto>, ApiError> {
    data.with_store(|store: &mut Store| {
        // 书名一次查完（不为一本书跑一趟）。书架列表本身就是「未删除的作品」，
        // 与检索那边"删除的不出现"是同一条口径。
        let titles: HashMap<i64, String> = store
            .list_works()?
            .into_iter()
            .map(|work| (work.id, work.title))
            .collect();
        Ok(store
            .search(&query, work_id, limit)?
            .into_iter()
            .map(|hit| to_dto(hit, &titles))
            .collect())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(work_id: i64) -> SearchHit {
        SearchHit {
            node_id: 7,
            work_id,
            title: "第 1 章".into(),
            snippet: "……".into(),
            matched_title: false,
        }
    }

    #[test]
    fn the_book_name_comes_from_the_shelf() {
        let titles = HashMap::from([(1_i64, "长夜".to_string())]);
        let dto = to_dto(hit(1), &titles);
        assert_eq!(dto.work_title, "长夜");
        assert_eq!(dto.node_id, 7);
        assert_eq!(dto.title, "第 1 章");
    }

    #[test]
    fn a_missing_or_empty_book_name_stays_empty() {
        // 两种都得退化成空串：作者还没给书起名、书架上找不到这一本（理论上不该发生——
        // 但界面能自己给占位，壳**不替作者编名字**）。
        let titles = HashMap::from([(2_i64, String::new())]);
        assert_eq!(to_dto(hit(2), &titles).work_title, "");
        assert_eq!(to_dto(hit(9), &titles).work_title, "");
    }
}
