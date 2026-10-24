//! 来源账的**运行段**（runs）：正文按字节切成一串 `[start, end) → 来源`。
//!
//! 这是"这一章的字是从哪来的"的唯一存储形状：
//!
//! - **字节下标**（不是字符下标）：切片是 O(1)，而且不会把多字节字符切坏；
//!   渲染字数时才按字符算（见 [`tally_by_source`](crate::text::tally_by_source)）。
//! - **段永远铺满整篇正文**：`runs` 从 0 连到 `body.len()`，中间不许有洞。
//!   有洞就意味着"这一段是谁写的连记录都没有"，那本身就是一种来源（`unknown`），
//!   该显式写出来，不该靠"读不到"来暗示。
//! - **相邻同源合并**：一边打字一边落盘会产生大量碎段，不合并的话一次会话就能攒出几百段。
//!
//! 存储形状是 JSON 三元组 `[[start, end, "typed"], …]`——人能读、坏了好查，
//! 而且比自定义分隔符少一类"分隔符出现在内容里"的坑（这里本来没有内容，只有码）。

use crate::model::EditOrigin;

/// 一段：从 `start` 到 `end`（字节，左闭右开）是 `origin` 来的。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Run {
    pub start: usize,
    pub end: usize,
    pub origin: EditOrigin,
}

impl Run {
    fn new(start: usize, end: usize, origin: EditOrigin) -> Self {
        Self { start, end, origin }
    }
}

/// 新建一段正文时的整段来源（例：把一条**口述**记下的灵感建成场景卡——
/// 那一段字不是今天新打的，但它的来源是知道的，不该记成「未记录」）。
pub(super) fn seeded(body: &str, origin: EditOrigin) -> Vec<Run> {
    if body.is_empty() {
        Vec::new()
    } else {
        vec![Run::new(0, body.len(), origin)]
    }
}

/// 解析库里的那一份。**坏记录一律当"没有记录"**（`None`）——不猜、不半信半疑。
///
/// 认不出的来源码**不丢弃整份账**，只把这一段记成"未记录"：多一个未来才有的码，
/// 不该让作者整章的来源全丢掉（那比少一档更坏）。
pub(super) fn decode(json: &str) -> Option<Vec<Run>> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let items = value.as_array()?;
    let mut runs = Vec::with_capacity(items.len());
    for item in items {
        let triple = item.as_array()?;
        if triple.len() != 3 {
            return None;
        }
        let start = usize::try_from(triple[0].as_u64()?).ok()?;
        let end = usize::try_from(triple[1].as_u64()?).ok()?;
        if start >= end {
            return None;
        }
        let origin = triple[2]
            .as_str()
            .map(|code| EditOrigin::parse(code).unwrap_or(EditOrigin::Unknown))?;
        runs.push(Run::new(start, end, origin));
    }
    Some(runs)
}

/// 写成库里的那一份。
pub(super) fn encode(runs: &[Run]) -> String {
    let items: Vec<[serde_json::Value; 3]> = runs
        .iter()
        .map(|run| {
            [
                serde_json::Value::from(run.start),
                serde_json::Value::from(run.end),
                serde_json::Value::from(run.origin.as_str()),
            ]
        })
        .collect();
    serde_json::Value::from(items).to_string()
}

/// 整理成**铺满 `len`、有序、无重叠、相邻同源合并**的一串。
///
/// 缺口补 `unknown`：宁可如实说"这一段没记录"，也不要把它偷偷算进旁边的来源。
pub(super) fn normalize(runs: &[Run], len: usize) -> Vec<Run> {
    if len == 0 {
        return Vec::new();
    }
    let mut sorted: Vec<Run> = runs
        .iter()
        .filter(|run| run.start < len && run.end > 0)
        .map(|run| Run::new(run.start.min(len), run.end.min(len), run.origin))
        .filter(|run| run.start < run.end)
        .collect();
    sorted.sort_by_key(|run| (run.start, run.end));

    let mut out: Vec<Run> = Vec::with_capacity(sorted.len() + 1);
    let mut cursor = 0usize;
    for run in sorted {
        let start = run.start.max(cursor);
        if start > cursor {
            push(&mut out, cursor, start, EditOrigin::Unknown);
        }
        if run.end > start {
            push(&mut out, start, run.end, run.origin);
            cursor = run.end;
        }
    }
    if cursor < len {
        push(&mut out, cursor, len, EditOrigin::Unknown);
    }
    out
}

fn push(out: &mut Vec<Run>, start: usize, end: usize, origin: EditOrigin) {
    if start >= end {
        return;
    }
    if let Some(last) = out.last_mut() {
        if last.origin == origin && last.end == start {
            last.end = end;
            return;
        }
    }
    out.push(Run::new(start, end, origin));
}

/// 把旧正文里 `[from, to)` 这一段搬进新位置（`shift` = 新起点 − 旧起点，可为负）。
///
/// 只搬与这一段相交的部分；返回的段**不整理**，由调用方最后统一 [`normalize`]。
pub(super) fn copy_range(
    runs: &[Run],
    from: usize,
    to: usize,
    shift: isize,
    out: &mut Vec<Run>,
) {
    if from >= to {
        return;
    }
    for run in runs {
        let start = run.start.max(from);
        let end = run.end.min(to);
        if start >= end {
            continue;
        }
        let moved_start = (start as isize + shift).max(0) as usize;
        let moved_end = (end as isize + shift).max(0) as usize;
        if moved_start < moved_end {
            out.push(Run::new(moved_start, moved_end, run.origin));
        }
    }
}

/// 拆章：把一串段按切点分成前半与后半（后半的坐标从 0 重新算）。
///
/// `head_len` 是前半**保留下来**的长度（`trim_end` 之后），`cut` 是后半的起点。
/// 切点处被收掉的空白没有字，不给它编来源。
pub(super) fn split_at(runs: &[Run], cut: usize, head_len: usize, body_len: usize) -> (Vec<Run>, Vec<Run>) {
    let mut head = Vec::new();
    copy_range(runs, 0, head_len, 0, &mut head);
    let mut tail = Vec::new();
    copy_range(runs, cut, body_len, -(cut as isize), &mut tail);
    (normalize(&head, head_len), normalize(&tail, body_len.saturating_sub(cut)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runs(pairs: &[(usize, usize, EditOrigin)]) -> Vec<Run> {
        pairs.iter().map(|(s, e, o)| Run::new(*s, *e, *o)).collect()
    }

    #[test]
    fn encode_decode_round_trip() {
        let list = runs(&[(0, 3, EditOrigin::Typed), (3, 8, EditOrigin::Pasted)]);
        let json = encode(&list);
        assert_eq!(decode(&json).unwrap(), list);
    }

    #[test]
    fn malformed_records_are_treated_as_no_record() {
        for json in ["", "not json", "{}", "[[0,3]]", "[[3,0,\"typed\"]]", "[[0,3,4]]"] {
            assert!(decode(json).is_none(), "坏记录必须当没有记录：{json}");
        }
    }

    #[test]
    fn unknown_future_codes_degrade_that_run_only() {
        let decoded = decode("[[0,3,\"typed\"],[3,6,\"hologram\"]]").unwrap();
        assert_eq!(decoded[1].origin, EditOrigin::Unknown);
    }

    #[test]
    fn normalize_fills_gaps_and_merges_neighbours() {
        let messy = runs(&[
            (8, 10, EditOrigin::Typed),
            (0, 4, EditOrigin::Typed),
            (4, 6, EditOrigin::Typed),
            (6, 8, EditOrigin::Pasted),
        ]);
        let out = normalize(&messy, 12);
        assert_eq!(
            out,
            runs(&[
                (0, 6, EditOrigin::Typed),
                (6, 8, EditOrigin::Pasted),
                (8, 10, EditOrigin::Typed),
                (10, 12, EditOrigin::Unknown),
            ])
        );
        assert!(out.windows(2).all(|pair| pair[0].end == pair[1].start), "段必须首尾相接");
    }

    #[test]
    fn empty_body_has_no_runs() {
        assert!(normalize(&runs(&[(0, 5, EditOrigin::Typed)]), 0).is_empty());
    }

    #[test]
    fn split_keeps_each_half_s_own_sources() {
        // "甲甲甲\n\n乙乙乙"：前 3 字键入、空行未记录、后 3 字粘贴
        let body = "甲甲甲\n\n乙乙乙";
        let cut = body.find('乙').unwrap();
        let list = normalize(
            &runs(&[(0, cut, EditOrigin::Typed), (cut, body.len(), EditOrigin::Pasted)]),
            body.len(),
        );
        let head_len = body[..cut].trim_end().len();
        let (head, tail) = split_at(&list, cut, head_len, body.len());
        assert_eq!(head.iter().map(|r| r.end - r.start).sum::<usize>(), head_len);
        assert_eq!(tail.iter().map(|r| r.end - r.start).sum::<usize>(), body.len() - cut);
        assert_eq!(tail[0].origin, EditOrigin::Pasted);
        assert_eq!(head.last().unwrap().origin, EditOrigin::Typed);
    }
}
