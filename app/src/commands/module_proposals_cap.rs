//! 建议的**每日审阅额度**：模块能自由往自己数据区写建议，但"摆到作者眼前"由壳把关。
//!
//! 超额不是错误：只摆出剩下的名额，其余**如实计数**（`suppressed`），不静默吞掉。

use crate::error::ApiError;
use crate::storage::AppData;

use yanmo_core::store::ModuleData;

use super::module_proposals::ProposalView;

/// 今日审阅额度：只数"今天记过结果"的条数（每条建议无论应用还是被忽略，都算占过一次名额）。
pub(crate) fn reviewed_today(data: &AppData, module_id: &str) -> Result<usize, ApiError> {
    let module_data = ModuleData::open(ModuleData::path_beside(
        &data.data_dir().join("yanmo.db"),
    ))?;
    let Some(bytes) = module_data.kv_get(module_id, "proposal_results.json")? else {
        return Ok(0);
    };
    let Ok(records) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return Ok(0);
    };
    let now = yanmo_core::time::now_millis();
    let day_start = now - now.rem_euclid(24 * 60 * 60 * 1000);
    Ok(records
        .as_array()
        .map(|list| {
            list.iter()
                .filter(|record| {
                    record
                        .get("at")
                        .and_then(|value| value.as_i64())
                        .map(|at| at >= day_start)
                        .unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0))
}

/// 额度用完了就只摆出剩下的名额（其余**如实计数**，不静默吞掉）。
pub(crate) fn limit_daily(
    items: Vec<ProposalView>,
    reviewed_today: usize,
    cap: usize,
) -> (Vec<ProposalView>, usize) {
    let left = cap.saturating_sub(reviewed_today);
    if items.len() <= left {
        return (items, 0);
    }
    let mut items = items;
    let suppressed = items.len() - left;
    items.truncate(left);
    (items, suppressed)
}

#[cfg(test)]
mod rate_tests {
    use super::*;

    fn item(id: &str) -> ProposalView {
        ProposalView {
            id: id.to_string(),
            node_id: 1,
            title: String::new(),
            base_hash: String::new(),
            stale: false,
            chars: 0,
            kind: "text".to_string(),
            parent: None,
            index: 0,
            nodes: Vec::new(),
            target: None,
            source: None,
            source_hash: String::new(),
        }
    }

    #[test]
    fn the_daily_cap_only_shows_what_is_left_and_counts_the_rest() {
        let items: Vec<ProposalView> = (0..5).map(|i| item(&format!("p{i}"))).collect();
        let (shown, suppressed) = limit_daily(items, 3, 5);
        assert_eq!(shown.len(), 2, "three already reviewed today leaves two slots");
        assert_eq!(suppressed, 3);
        let (shown, suppressed) = limit_daily(vec![item("a")], 5, 5);
        assert!(shown.is_empty());
        assert_eq!(suppressed, 1);
        let (shown, suppressed) = limit_daily(vec![item("a")], 0, 5);
        assert_eq!(shown.len(), 1);
        assert_eq!(suppressed, 0);
    }
}
