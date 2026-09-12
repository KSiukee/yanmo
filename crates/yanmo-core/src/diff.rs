//! 行级差异（统一 diff）：**"这一版和现在差在哪"** 的唯一算法落点。
//!
//! 这里只给**结构化**结果（每行是"相同 / 新增 / 删除 / 折叠"），一句界面文案都不产出——
//! 核心零 UI 依赖（见 `lib.rs`），画成红绿两色是界面的事。
//!
//! # 为什么不引第三方 diff 库
//!
//! 我们的用法很窄（同一章的两版、几百行、只给人看），而许可与体积都要过一遍红线
//! （见规则 16）。核心里的这份实现是纯函数、可单测、无依赖，足够。
//!
//! # 算法
//!
//! 1. 先剥掉**公共前后缀行**：改一版通常只动中间几段，这一步就让绝大多数比对退化成
//!    "几行差异 + 两端折叠"；
//! 2. 中间块做 LCS（动态规划）；规模超 [`MAX_DP_CELLS`] 时**不做逐行对齐**，
//!    整块标成"删 + 增"并置 `truncated`——宁可说清"只给到这一层"，也不拿内存去赌。
//!
//! 行 = 以 `\n` 切分（一行就是一个段落，与编辑器 `docToText` 的口径一致）；空文档算 0 行。

/// 中间块 LCS 的规模上限（格子数）。2000 行 × 2000 行 ≈ 16MB，是这里愿意付的上限。
const MAX_DP_CELLS: usize = 4_000_000;

/// 一行差异的种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    /// 两版都有
    Same,
    /// 只在较新那一版里
    Added,
    /// 只在较旧那一版里
    Removed,
    /// 被折叠掉的一串相同行（`hidden` 是被折掉的行数）
    Skipped,
}

/// 差异里的一行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffKind,
    /// 行内容（`Skipped` 为空串——它不代表某一行）
    pub text: String,
    /// 在旧版里的行号（从 1 起；`Added` 为 None）
    pub old_line: Option<usize>,
    /// 在新版里的行号（从 1 起；`Removed` 为 None）
    pub new_line: Option<usize>,
    /// 被折叠的相同行数（只有 `Skipped` 非 0）
    pub hidden: usize,
}

/// 一次比对的完整结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffReport {
    /// 逐行差异（相同的行已按上下文折叠）；**两版完全一样时为空**
    pub lines: Vec<DiffLine>,
    pub added: usize,
    pub removed: usize,
    /// 规模超限，中间块没有逐行对齐（界面要说清这一点）
    pub truncated: bool,
}

/// 比对两版正文（`context` = 差异行前后各留几行相同的上下文）。
pub fn diff_lines(old: &str, new: &str, context: usize) -> DiffReport {
    compare(old, new, context, MAX_DP_CELLS)
}

fn compare(old: &str, new: &str, context: usize, max_cells: usize) -> DiffReport {
    let old_lines = lines_of(old);
    let new_lines = lines_of(new);

    let mut prefix = 0;
    while prefix < old_lines.len() && prefix < new_lines.len() && old_lines[prefix] == new_lines[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < old_lines.len() - prefix
        && suffix < new_lines.len() - prefix
        && old_lines[old_lines.len() - 1 - suffix] == new_lines[new_lines.len() - 1 - suffix]
    {
        suffix += 1;
    }

    let mut rows: Vec<DiffLine> = Vec::new();
    for index in 0..prefix {
        rows.push(same(old_lines[index], index, index));
    }

    let mid_old = &old_lines[prefix..old_lines.len() - suffix];
    let mid_new = &new_lines[prefix..new_lines.len() - suffix];
    let mut truncated = false;
    if !mid_old.is_empty() && !mid_new.is_empty() && mid_old.len() * mid_new.len() > max_cells {
        // 太大：不做逐行对齐，整块"删 + 增"。说清降级比悄悄吃掉内存好
        truncated = true;
        for (offset, text) in mid_old.iter().enumerate() {
            rows.push(removed(text, prefix + offset));
        }
        for (offset, text) in mid_new.iter().enumerate() {
            rows.push(added(text, prefix + offset));
        }
    } else {
        lcs_rows(mid_old, mid_new, prefix, &mut rows);
    }

    for offset in 0..suffix {
        rows.push(same(
            old_lines[old_lines.len() - suffix + offset],
            old_lines.len() - suffix + offset,
            new_lines.len() - suffix + offset,
        ));
    }

    let added = rows.iter().filter(|row| row.kind == DiffKind::Added).count();
    let removed = rows.iter().filter(|row| row.kind == DiffKind::Removed).count();
    let lines = if added == 0 && removed == 0 { Vec::new() } else { fold(rows, context) };
    DiffReport { lines, added, removed, truncated }
}

/// 中间块做 LCS，回溯出"相同 / 删 / 增"三种行。
///
/// 相同行按**旧版优先**回溯（`dp[i+1][j] >= dp[i][j+1]`）：差异会先显示成删除、
/// 再显示成新增——这正是统一 diff 的读法（"原来那句没了，换成了这句"）。
fn lcs_rows(mid_old: &[&str], mid_new: &[&str], base: usize, out: &mut Vec<DiffLine>) {
    let (n, m) = (mid_old.len(), mid_new.len());
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if mid_old[i] == mid_new[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if mid_old[i] == mid_new[j] {
            out.push(same(mid_old[i], base + i, base + j));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            out.push(removed(mid_old[i], base + i));
            i += 1;
        } else {
            out.push(added(mid_new[j], base + j));
            j += 1;
        }
    }
    for (offset, text) in mid_old[i..].iter().enumerate() {
        out.push(removed(text, base + i + offset));
    }
    for (offset, text) in mid_new[j..].iter().enumerate() {
        out.push(added(text, base + j + offset));
    }
}

/// 把离差异太远的相同行折起来：差异行前后各留 `context` 行。
fn fold(rows: Vec<DiffLine>, context: usize) -> Vec<DiffLine> {
    let mut keep = vec![false; rows.len()];
    for (index, row) in rows.iter().enumerate() {
        if row.kind == DiffKind::Same {
            continue;
        }
        let from = index.saturating_sub(context);
        let to = (index + context).min(rows.len() - 1);
        keep[from..=to].fill(true);
    }

    let mut out = Vec::new();
    let mut index = 0;
    while index < rows.len() {
        if keep[index] {
            out.push(rows[index].clone());
            index += 1;
            continue;
        }
        let start = index;
        while index < rows.len() && !keep[index] {
            index += 1;
        }
        // 被折的这段一定是相同行（差异行自己一定被 keep）——记下它从哪开始，界面可显示"跳过 N 行"
        out.push(DiffLine {
            kind: DiffKind::Skipped,
            text: String::new(),
            old_line: rows[start].old_line,
            new_line: rows[start].new_line,
            hidden: index - start,
        });
    }
    out
}

/// 空文档算 0 行；否则按 `\n` 切（结尾的换行不额外算一行）。
fn lines_of(text: &str) -> Vec<&str> {
    if text.is_empty() {
        Vec::new()
    } else {
        text.split('\n').collect()
    }
}

fn same(text: &str, old_index: usize, new_index: usize) -> DiffLine {
    DiffLine {
        kind: DiffKind::Same,
        text: text.to_string(),
        old_line: Some(old_index + 1),
        new_line: Some(new_index + 1),
        hidden: 0,
    }
}

fn added(text: &str, new_index: usize) -> DiffLine {
    DiffLine {
        kind: DiffKind::Added,
        text: text.to_string(),
        old_line: None,
        new_line: Some(new_index + 1),
        hidden: 0,
    }
}

fn removed(text: &str, old_index: usize) -> DiffLine {
    DiffLine {
        kind: DiffKind::Removed,
        text: text.to_string(),
        old_line: Some(old_index + 1),
        new_line: None,
        hidden: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_text_has_no_rows() {
        let report = diff_lines("第一段\n第二段", "第一段\n第二段", 2);
        assert!(report.lines.is_empty(), "两版一样时不该有差异行");
        assert_eq!((report.added, report.removed), (0, 0));
        assert!(!report.truncated);
    }

    #[test]
    fn changed_middle_line_shows_removed_then_added() {
        let old = "一\n二\n三\n四\n五";
        let new = "一\n二\n三改\n四\n五";
        let report = diff_lines(old, new, 1);
        let shown: Vec<(DiffKind, &str)> =
            report.lines.iter().map(|row| (row.kind, row.text.as_str())).collect();
        assert_eq!(
            shown,
            vec![
                (DiffKind::Skipped, ""), // 上面那两行相同、离差异太远：折起来
                (DiffKind::Same, "二"),
                (DiffKind::Removed, "三"),
                (DiffKind::Added, "三改"),
                (DiffKind::Same, "四"),
                (DiffKind::Skipped, ""), // 下面那行相同：折起来
            ],
            "{:?}",
            report.lines
        );
        assert_eq!((report.added, report.removed), (1, 1));
        let skipped: Vec<usize> =
            report.lines.iter().filter(|r| r.kind == DiffKind::Skipped).map(|r| r.hidden).collect();
        assert_eq!(skipped, vec![1, 1], "两端各折掉一行相同行");
    }

    #[test]
    fn added_paragraph_keeps_line_numbers() {
        let report = diff_lines("一\n三", "一\n二\n三", 3);
        let added = report.lines.iter().find(|r| r.kind == DiffKind::Added).unwrap();
        assert_eq!(added.text, "二");
        assert_eq!(added.new_line, Some(2));
        assert_eq!(added.old_line, None);
    }

    #[test]
    fn empty_documents_are_handled() {
        assert!(diff_lines("", "", 1).lines.is_empty());
        let report = diff_lines("", "新写的一段", 1);
        assert_eq!(report.added, 1);
        assert_eq!(report.removed, 0);
        assert_eq!(report.lines[0].kind, DiffKind::Added);
    }

    #[test]
    fn oversized_blocks_degrade_instead_of_eating_memory() {
        let old = (0..10).map(|i| format!("旧{i}")).collect::<Vec<_>>().join("\n");
        let new = (0..10).map(|i| format!("新{i}")).collect::<Vec<_>>().join("\n");
        let report = compare(&old, &new, 1, 4); // 上限调到 4 格：10×10 直接降级
        assert!(report.truncated, "超限要说清是降级结果");
        assert_eq!((report.added, report.removed), (10, 10), "降级 = 整块删 + 整块增");
    }
}
