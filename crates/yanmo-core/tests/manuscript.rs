//! 稿件的**状态可达性**与投稿判据验收。
//!
//! 这一份是"尺子"：它不测某个函数写得对不对，而是机械地验三件事——
//!
//! 1. **进得来**：七个态**各自**有一条从草稿出发的真实路径，且路径上每一步都是合法边；
//! 2. **出得去**：只有 `published` 没有出边，其余态都有，并且"被拒 → 冷冻期结束 → 可改投"
//!    这条**重出路径**真的存在（不是靠读代码印象）；
//! 3. **有据可查**：路径上的每一步都能报出一个 op-log 动作码（落库留痕见 `store::manuscript`）。
//!
//! 纯逻辑那一半在这里；界面侧那一半（真拖拽）走真机走查。

use yanmo_core::model::{
    manuscript_transition, submission_verdict, ManuscriptState, Submission, SubmissionResult,
    MANUSCRIPT_TRANSITIONS,
};

const NOW: i64 = 1_000_000;

fn sub(venue: &str, result: SubmissionResult, cooldown_until: Option<i64>) -> Submission {
    Submission {
        id: 1,
        work_id: 7,
        venue: venue.to_string(),
        submitted_at: 1_000,
        result,
        cooldown_until,
        notes: String::new(),
    }
}

/// 每个态一条真实到达路径（从草稿出发）。
fn reachable_paths() -> Vec<(ManuscriptState, Vec<ManuscriptState>)> {
    use ManuscriptState::*;
    vec![
        (Draft, vec![Draft]),
        (Finished, vec![Draft, Finished]),
        (Ready, vec![Draft, Ready]),
        (Submitted, vec![Draft, Ready, Submitted]),
        (Accepted, vec![Draft, Ready, Submitted, Accepted]),
        (Rejected, vec![Draft, Ready, Submitted, Rejected]),
        (Published, vec![Draft, Ready, Submitted, Accepted, Published]),
    ]
}

#[test]
fn every_state_has_a_real_path_from_draft() {
    let paths = reachable_paths();
    for state in ManuscriptState::ALL {
        let (_, path) = paths
            .iter()
            .find(|(target, _)| *target == state)
            .unwrap_or_else(|| panic!("{} 没有登记到达路径", state.as_str()));
        assert_eq!(path[0], ManuscriptState::initial(), "每条路径都要从草稿出发");
        assert_eq!(path.last().copied(), Some(state), "路径终点得是它自己");
        for step in path.windows(2) {
            let edge = manuscript_transition(step[0], step[1])
                .unwrap_or_else(|| panic!("非法路径：{} → {}", step[0].as_str(), step[1].as_str()));
            assert!(!edge.action.is_empty(), "每条边都要有落库用的动作码");
        }
    }
}

/// 看板列顺序 = `ALL` 顺序：界面不再自己排一遍（两份口径迟早分家）。
#[test]
fn board_column_order_is_the_declared_order() {
    let cols: Vec<&str> = ManuscriptState::ALL.iter().map(|s| s.as_str()).collect();
    assert_eq!(
        cols,
        vec!["draft", "finished", "ready", "submitted", "accepted", "rejected", "published"]
    );
}

#[test]
fn the_main_path_speaks_in_stable_action_codes() {
    use ManuscriptState::*;
    let path = [Draft, Ready, Submitted, Accepted, Published];
    let actions: Vec<&str> =
        path.windows(2).map(|s| manuscript_transition(s[0], s[1]).unwrap().action).collect();
    assert_eq!(actions, vec!["ready", "submit", "accept", "publish"]);
}

/// 终态的入口要说得清：过稿从"已投"来、已发从"过稿"来。
#[test]
fn terminal_states_have_a_declared_entry() {
    let into = |to: ManuscriptState| {
        MANUSCRIPT_TRANSITIONS.iter().filter(|t| t.to == to).map(|t| t.from).collect::<Vec<_>>()
    };
    assert_eq!(into(ManuscriptState::Accepted), vec![ManuscriptState::Submitted]);
    assert_eq!(into(ManuscriptState::Published), vec![ManuscriptState::Accepted]);
}

/// **重出路径**：被拒不是终点——冷冻期结束可以改投别家。
///
/// 状态那一半：`被拒 --revive--> 待投 --submit--> 已投`；
/// 判据那一半：超期的等待记录不再拦住新投稿。
#[test]
fn rejected_piece_can_re_enter_after_cooldown() {
    use ManuscriptState::*;
    let revive = manuscript_transition(Rejected, Ready).expect("被拒必须出得去");
    assert_eq!(revive.action, "revive");
    assert!(manuscript_transition(Ready, Submitted).is_some());

    // 冷冻期已经过了：编辑没回音，视为可改投
    let stale = [sub("甲刊", SubmissionResult::Waiting, Some(NOW - 1))];
    let v = submission_verdict(&stale, "乙刊", NOW);
    assert!(v.is_clear(), "冷冻期结束后改投别家不该被拦：{v:?}");
}

/// 一稿多投在**没超期**时是真的拦得住（否则这把尺子只会给"永远 0 问题"的假绿）。
#[test]
fn active_submission_still_blocks_a_second_venue() {
    let history = [sub("甲刊", SubmissionResult::Waiting, Some(NOW + 1))];
    let v = submission_verdict(&history, "乙刊", NOW);
    assert_eq!(v.code(), "awaiting_reply");
    match v {
        yanmo_core::model::SubmitVerdict::AwaitingReply { venue, cooldown_until, .. } => {
            assert_eq!(venue, "甲刊", "要说得出在等谁");
            assert_eq!(cooldown_until, Some(NOW + 1), "也要说得出等到哪天");
        }
        other => panic!("判词不对：{other:?}"),
    }
}

/// 没定期限的等待也算"还在等"——不替作者假设他能改投。
#[test]
fn waiting_without_deadline_still_blocks() {
    let history = [sub("甲刊", SubmissionResult::Waiting, None)];
    assert_eq!(submission_verdict(&history, "乙刊", NOW).code(), "awaiting_reply");
}

/// 超期未回 → 换一家不再拦，但同一家仍提醒"投过"。
#[test]
fn stale_waiting_does_not_block_but_warns_if_same_venue() {
    let history = [sub("甲刊", SubmissionResult::Waiting, Some(NOW - 1))];
    assert!(submission_verdict(&history, "乙刊", NOW).is_clear());
    assert_eq!(submission_verdict(&history, "甲刊", NOW).code(), "already_tried");
}

#[test]
fn rejected_venue_cools_down_then_clears() {
    let cooling = [sub("甲刊", SubmissionResult::Rejected, Some(NOW + 1))];
    assert_eq!(submission_verdict(&cooling, "甲刊", NOW).code(), "venue_cooldown");
    // 冷冻期过了：同一家可以再投（作者自己决定）；换一家一直放行
    let passed = [sub("甲刊", SubmissionResult::Rejected, Some(NOW - 1))];
    assert_eq!(submission_verdict(&passed, "甲刊", NOW).code(), "already_tried");
    assert!(submission_verdict(&cooling, "乙刊", NOW).is_clear());
}

/// 一稿多投优先于"这家投过"：两样都占时先说不许一稿多投。
#[test]
fn awaiting_reply_outranks_already_tried() {
    let mut second = sub("乙刊", SubmissionResult::Rejected, None);
    second.id = 2;
    let history = [sub("甲刊", SubmissionResult::Waiting, Some(NOW + 1)), second];
    assert_eq!(submission_verdict(&history, "乙刊", NOW).code(), "awaiting_reply");
}

/// 刊物名按 trim 后全等比：不猜近似名（猜错了会拦错人，比不拦更糟）。
#[test]
fn venue_match_is_trimmed_exact() {
    let history = [sub("甲刊", SubmissionResult::Rejected, None)];
    assert_eq!(submission_verdict(&history, " 甲刊 ", NOW).code(), "already_tried");
    assert!(submission_verdict(&history, "甲刊（副刊）", NOW).is_clear());
}

#[test]
fn empty_history_is_clear() {
    assert!(submission_verdict(&[], "甲刊", NOW).is_clear());
}
