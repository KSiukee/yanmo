//! 一次正文改动落到来源账上：**旧段怎么搬、新字记给谁**。
//!
//! 核心只看得到"旧正文"与"新正文"（击键留在壳内，见 `store::content` 的说明），
//! 所以归属只能靠比对。算法分两层：
//!
//! 1. **按行对齐**：先剥掉首尾相同的行，中间那一块做 LCS。没变的行连段一起搬过去；
//!    变了的行**成对**处理（旧行 → 新行）。
//! 2. **行内按字符前后缀**：一对变了的行，公共前缀与公共后缀的段照样搬，
//!    中间那截才是这一笔新写的——记成这一次落盘的来源。打字打在段落中间、
//!    粘贴插在两句之间，切的都是这一层。
//!
//! # 诚实的边界
//!
//! 落盘窗口是 250ms 上下，一笔里几乎只可能有一处连续改动。**同一笔里若在两处
//! 相隔很远的地方同时改了**（先粘贴、又立刻跳到别处敲字），中间那一整块会被记成
//! "这一笔的来源"——比真实归属粗。这一档**没有更细的证据可用**（击键不跨进程），
//! 宁可记粗，也不假装知道每一段分别是谁写的。中间块过大时（超过
//! `MAX_DP_CELLS`）不做逐行对齐，整块按前后缀切——同样是"记粗"，且不拿内存去赌。

use super::source_run::{copy_range, normalize, Run};
use crate::model::EditOrigin;

/// 中间块做 LCS 的规模上限（格子数）。500×500 ≈ 1MB 的 u32 表，是这里愿意付的上限。
const MAX_DP_CELLS: usize = 250_000;

/// 一行：`end` **包含**行尾换行（最后一行没有换行）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Line {
    start: usize,
    end: usize,
}

fn lines(text: &str) -> Vec<Line> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            out.push(Line { start, end: index + 1 });
            start = index + 1;
        }
    }
    if start < text.len() {
        out.push(Line { start, end: text.len() });
    }
    out
}

fn same_line(old: &str, a: Line, new: &str, b: Line) -> bool {
    old[a.start..a.end] == new[b.start..b.end]
}

/// 把一次"旧正文 → 新正文"的改动记到来源账上，返回**铺满新正文**的一串段。
pub(super) fn apply(old_runs: &[Run], old: &str, new: &str, origin: EditOrigin) -> Vec<Run> {
    if old == new {
        return normalize(old_runs, new.len());
    }
    let old_lines = lines(old);
    let new_lines = lines(new);

    let mut prefix = 0usize;
    while prefix < old_lines.len()
        && prefix < new_lines.len()
        && same_line(old, old_lines[prefix], new, new_lines[prefix])
    {
        prefix += 1;
    }
    let mut suffix = 0usize;
    while suffix < old_lines.len() - prefix
        && suffix < new_lines.len() - prefix
        && same_line(
            old,
            old_lines[old_lines.len() - 1 - suffix],
            new,
            new_lines[new_lines.len() - 1 - suffix],
        )
    {
        suffix += 1;
    }

    let mut out: Vec<Run> = Vec::new();
    for index in 0..prefix {
        // 前缀行逐字相同：整段平移（这里位移是 0，但按同一条 copy_range 走，少一条特例）
        copy_range(
            old_runs,
            old_lines[index].start,
            old_lines[index].end,
            new_lines[index].start as isize - old_lines[index].start as isize,
            &mut out,
        );
    }

    let old_mid = &old_lines[prefix..old_lines.len() - suffix];
    let new_mid = &new_lines[prefix..new_lines.len() - suffix];
    align(old_runs, old, old_mid, new, new_mid, origin, &mut out);

    let shift = new.len() as isize - old.len() as isize;
    for index in 0..suffix {
        let old_line = old_lines[old_lines.len() - suffix + index];
        let new_line = new_lines[new_lines.len() - suffix + index];
        // 后缀行逐字相同：整段平移即可（`shift` 对这一段是常数）
        let local = new_line.start as isize - old_line.start as isize;
        debug_assert_eq!(local, shift);
        copy_range(old_runs, old_line.start, old_line.end, local, &mut out);
    }

    normalize(&out, new.len())
}

fn align(
    old_runs: &[Run],
    old: &str,
    old_mid: &[Line],
    new: &str,
    new_mid: &[Line],
    origin: EditOrigin,
    out: &mut Vec<Run>,
) {
    if old_mid.is_empty() {
        for line in new_mid {
            out.push(Run { start: line.start, end: line.end, origin });
        }
        return;
    }
    if new_mid.is_empty() {
        return;
    }
    if old_mid.len() * new_mid.len() <= MAX_DP_CELLS {
        align_lcs(old_runs, old, old_mid, new, new_mid, origin, out);
    } else {
        // 太大：不逐行对齐，按整块的字符前后缀切（记粗，但绝不吃内存）
        let old_start = old_mid[0].start;
        let new_start = new_mid[0].start;
        let old_text = &old[old_start..old_mid[old_mid.len() - 1].end];
        let new_text = &new[new_start..new_mid[new_mid.len() - 1].end];
        let (prefix, suffix) = common_affix(old_text, new_text);
        copy_range(old_runs, old_start, old_start + prefix, new_start as isize - old_start as isize, out);
        let new_end = new_start + new_text.len() - suffix;
        if new_start + prefix < new_end {
            out.push(Run { start: new_start + prefix, end: new_end, origin });
        }
        let old_tail = old_start + old_text.len() - suffix;
        let new_tail = new_start + new_text.len() - suffix;
        copy_range(old_runs, old_tail, old_start + old_text.len(), new_tail as isize - old_tail as isize, out);
    }
}

#[derive(Debug, Clone, Copy)]
enum Op {
    Match(usize, usize),
    Del(usize),
    Ins(usize),
}

fn align_lcs(
    old_runs: &[Run],
    old: &str,
    old_mid: &[Line],
    new: &str,
    new_mid: &[Line],
    origin: EditOrigin,
    out: &mut Vec<Run>,
) {
    let (n, m) = (old_mid.len(), new_mid.len());
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if same_line(old, old_mid[i], new, new_mid[j]) {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }

    let mut ops = Vec::with_capacity(n + m);
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if same_line(old, old_mid[i], new, new_mid[j]) {
            ops.push(Op::Match(i, j));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            ops.push(Op::Del(i));
            i += 1;
        } else {
            ops.push(Op::Ins(j));
            j += 1;
        }
    }
    while i < n {
        ops.push(Op::Del(i));
        i += 1;
    }
    while j < m {
        ops.push(Op::Ins(j));
        j += 1;
    }

    let mut k = 0usize;
    while k < ops.len() {
        if let Op::Match(oi, ni) = ops[k] {
            copy_range(
                old_runs,
                old_mid[oi].start,
                old_mid[oi].end,
                new_mid[ni].start as isize - old_mid[oi].start as isize,
                out,
            );
            k += 1;
            continue;
        }
        // 一段"删 + 增"：按先后成对，当成"这一行被改成了那一行"
        let mut dels = Vec::new();
        let mut inss = Vec::new();
        while k < ops.len() {
            match ops[k] {
                Op::Match(..) => break,
                Op::Del(oi) => dels.push(oi),
                Op::Ins(ni) => inss.push(ni),
            }
            k += 1;
        }
        let pairs = dels.len().min(inss.len());
        for at in 0..pairs {
            modified_pair(old_runs, old, old_mid[dels[at]], new, new_mid[inss[at]], origin, out);
        }
        for ni in &inss[pairs..] {
            let line = new_mid[*ni];
            out.push(Run { start: line.start, end: line.end, origin });
        }
        // 多出来的旧行就是被删掉了：它们的段自然不再出现
    }
}

/// 一行被改成了另一行：公共前后缀的段搬过去，中间那截记成本次来源。
fn modified_pair(
    old_runs: &[Run],
    old: &str,
    old_line: Line,
    new: &str,
    new_line: Line,
    origin: EditOrigin,
    out: &mut Vec<Run>,
) {
    let old_text = &old[old_line.start..old_line.end];
    let new_text = &new[new_line.start..new_line.end];
    let (prefix, suffix) = common_affix(old_text, new_text);

    if prefix > 0 {
        copy_range(old_runs, old_line.start, old_line.start + prefix, new_line.start as isize - old_line.start as isize, out);
    }
    let middle_start = new_line.start + prefix;
    let middle_end = new_line.end - suffix;
    if middle_start < middle_end {
        out.push(Run { start: middle_start, end: middle_end, origin });
    }
    if suffix > 0 {
        let old_tail = old_line.end - suffix;
        let new_tail = new_line.end - suffix;
        copy_range(old_runs, old_tail, old_line.end, new_tail as isize - old_tail as isize, out);
    }
}

/// 公共前缀与公共后缀（字节，落在字符边界上；两段不重叠）。
fn common_affix(a: &str, b: &str) -> (usize, usize) {
    let (ab, bb) = (a.as_bytes(), b.as_bytes());
    let mut prefix = 0usize;
    while prefix < a.len() && prefix < b.len() && ab[prefix] == bb[prefix] {
        prefix += 1;
    }
    while prefix > 0 && (!a.is_char_boundary(prefix) || !b.is_char_boundary(prefix)) {
        prefix -= 1;
    }
    let max = a.len().min(b.len()) - prefix;
    let mut suffix = 0usize;
    while suffix < max && ab[a.len() - 1 - suffix] == bb[b.len() - 1 - suffix] {
        suffix += 1;
    }
    while suffix > 0 && (!a.is_char_boundary(a.len() - suffix) || !b.is_char_boundary(b.len() - suffix)) {
        suffix -= 1;
    }
    (prefix, suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: EditOrigin = EditOrigin::Typed;
    const P: EditOrigin = EditOrigin::Pasted;

    fn seeded(body: &str, origin: EditOrigin) -> Vec<Run> {
        normalize(&[Run { start: 0, end: body.len(), origin }], body.len())
    }

    fn origin_at(runs: &[Run], at: usize) -> EditOrigin {
        runs.iter().find(|run| run.start <= at && at < run.end).map(|run| run.origin).unwrap()
    }

    fn covers(runs: &[Run], len: usize) {
        assert_eq!(runs.first().map(|r| r.start), if len == 0 { None } else { Some(0) });
        assert_eq!(runs.last().map(|r| r.end), if len == 0 { None } else { Some(len) });
        assert!(runs.windows(2).all(|pair| pair[0].end == pair[1].start), "段必须首尾相接");
        assert!(runs.iter().all(|run| run.start < run.end), "不许有空段");
    }

    #[test]
    fn typing_at_the_end_keeps_old_sources_and_marks_new_chars() {
        let old = "旧稿第一段。";
        let new = "旧稿第一段。新敲的一句。";
        let runs = apply(&seeded(old, T), old, new, T);
        covers(&runs, new.len());
        assert_eq!(origin_at(&runs, new.len() - 1), T);
        assert_eq!(runs.len(), 1, "同一来源相邻要合并");
    }

    #[test]
    fn a_paste_in_the_middle_is_the_only_pasted_range() {
        let old = "开头一句。结尾一句。";
        let pasted = "粘进来的一大段，来源不明。";
        let new = format!("开头一句。{pasted}结尾一句。");
        let runs = apply(&seeded(old, T), old, &new, P);
        covers(&runs, new.len());
        let at = new.find(pasted).unwrap();
        assert_eq!(origin_at(&runs, at), P);
        assert_eq!(origin_at(&runs, at + pasted.len() - 1), P);
        assert_eq!(origin_at(&runs, 0), T, "粘贴前面的字还是键入");
        assert_eq!(origin_at(&runs, new.len() - 1), T, "粘贴后面的字还是键入");
    }

    #[test]
    fn typing_inside_a_pasted_paragraph_splits_it() {
        let pasted = "粘贴进来的整段话。";
        let old = format!("前缀。{pasted}");
        let typed = "后来又补的字。";
        let new = format!("前缀。粘贴进来的{typed}整段话。");
        let at = old.find(pasted).unwrap();
        let runs = apply(
            &normalize(&[Run { start: 0, end: at, origin: T }, Run { start: at, end: old.len(), origin: P }], old.len()),
            &old,
            &new,
            T,
        );
        covers(&runs, new.len());
        let typed_at = new.find(typed).unwrap();
        assert_eq!(origin_at(&runs, typed_at), T);
        assert_eq!(origin_at(&runs, typed_at - 1), P);
        assert_eq!(origin_at(&runs, typed_at + typed.len()), P);
    }

    #[test]
    fn deleting_a_paragraph_drops_its_chars() {
        let old = "第一段。\n\n要删掉的这一段。\n\n第三段。";
        let new = "第一段。\n\n第三段。";
        let runs = apply(&seeded(old, T), old, new, T);
        covers(&runs, new.len());
        assert_eq!(runs.iter().map(|run| run.end - run.start).sum::<usize>(), new.len());
    }

    #[test]
    fn two_far_apart_edits_keep_the_untouched_middle() {
        let old = "一。\n\n中间不动的一段。\n\n三。";
        let new = "一改。\n\n中间不动的一段。\n\n三改。";
        let runs = apply(&seeded(old, P), old, new, T);
        covers(&runs, new.len());
        let middle = new.find("中间不动的一段").unwrap();
        assert_eq!(origin_at(&runs, middle), P, "没动过的中间段不该记成这一次的来源");
        assert_eq!(origin_at(&runs, new.find('改').unwrap()), T, "改的那一个字是这一笔键入的");
        assert_eq!(origin_at(&runs, new.rfind('改').unwrap()), T);
    }

    #[test]
    fn empty_old_body_marks_everything_with_this_write() {
        let new = "第一句。\n\n第二句。";
        let runs = apply(&[], "", new, P);
        covers(&runs, new.len());
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].origin, P);
    }

    #[test]
    fn a_huge_rewrite_degrades_to_affix_splitting_without_panicking() {
        let old = (0..900).map(|i| format!("旧{i}行")).collect::<Vec<_>>().join("\n");
        let new = (0..900).map(|i| format!("新{i}行")).collect::<Vec<_>>().join("\n");
        let runs = apply(&seeded(&old, T), &old, &new, P);
        covers(&runs, new.len());
        assert_eq!(runs[0].origin, P, "整块换掉：新内容记成这一次的来源");
    }

    #[test]
    fn identical_text_keeps_the_ledger_exactly() {
        let body = "一个字都不动。";
        let before = seeded(body, P);
        let after = apply(&before, body, body, T);
        assert_eq!(before, after);
    }
}
