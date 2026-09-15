//! 叩问「问题卡」验收：**模型 + 完整状态机 + 存储**（碎片统一表）。
//!
//! 这一份盯四件事，每一件都能被机械地判：
//!
//! 1. **进得来**：六个态都能从 `pending` 走到（不留"永远到不了的态"）；
//! 2. **出得去**：只有显式声明的终态（`answered`）可以没有出边；
//! 3. **有据可查**：每条迁移都能被**真实 API** 触发，并在两处留下证据——
//!    字段（`fragments.status`）与事件（op-log 的 `op`/`from`/`to`/`trigger`）；
//! 4. **不许静默**：非法边、不存在的卡、越界的重要度、跨书的溯源——都当场报错且一个字节不写。
//!
//! 判据不写死"经过哪些态"：它直接遍历 [`TRANSITIONS`]（迁移表的唯一真相源），
//! 所以**加一条边就会自动被这里验到**，不需要人来补测试。

use yanmo_core::model::{
    transition, NewQuestionCard, QuestionState, WorkKind, TRANSITIONS,
};
use yanmo_core::store::Store;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

fn new_card(work_id: i64, body: &str) -> NewQuestionCard {
    NewQuestionCard {
        work_id,
        body: body.to_string(),
        source: "core".to_string(),
        template_key: String::new(),
        importance: 0.5,
        linked: Vec::new(),
        derived_from: None,
        auto_derived: false,
    }
}

/// 库里那一行的状态字段（**证据落在哪张表/哪个字段**，直接读回来对）。
fn status_in_db(store: &Store, card_id: i64) -> String {
    store
        .conn()
        .query_row("SELECT status FROM fragments WHERE id = ?1", [card_id], |r| r.get(0))
        .unwrap()
}

fn op_rows(store: &Store, card_id: i64) -> Vec<(String, String)> {
    let mut stmt = store
        .conn()
        .prepare("SELECT op, payload FROM op_log WHERE entity='fragments' AND entity_id=?1 ORDER BY seq")
        .unwrap();
    let rows = stmt.query_map([card_id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    rows.map(|row| row.unwrap()).collect()
}

/// 从 `pending` 走到 `target` 的一条路径（走迁移表本身，DFS；边都合法）。
fn path_to(target: QuestionState) -> Vec<QuestionState> {
    let mut stack = vec![(QuestionState::Pending, Vec::new())];
    let mut seen = vec![QuestionState::Pending];
    while let Some((state, path)) = stack.pop() {
        if state == target {
            return path;
        }
        for edge in TRANSITIONS.iter().filter(|t| t.from == state) {
            if !seen.contains(&edge.to) {
                seen.push(edge.to);
                let mut next = path.clone();
                next.push(edge.to);
                stack.push((edge.to, next));
            }
        }
    }
    panic!("从 pending 走不到 {}——「进得来」这条不变量破了", target.as_str());
}

/// 把一张新卡开到 `target` 态；返回它经过的步数（每步都留了痕）。
fn drive_to(store: &mut Store, card_id: i64, target: QuestionState) {
    for next in path_to(target) {
        let from = store.move_question_card(card_id, next, "test:path").unwrap();
        assert!(
            transition(from, next).is_some(),
            "路径上每一步都该是合法边：{} → {}",
            from.as_str(),
            next.as_str()
        );
    }
    assert_eq!(store.question_card(card_id).unwrap().state, target);
}

#[test]
fn card_lands_in_the_shared_fragment_table() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "题本").unwrap();
    let id = store
        .create_question_card(&NewQuestionCard {
            template_key: "foreshadow.due".to_string(),
            importance: 0.8,
            ..new_card(work.id, "第 12 章埋下的信物，现在该让它露头了吗？")
        })
        .unwrap();

    // 统一表：问题卡是 `frag_kind = 'question'` 的一行，不是另起的表
    let (kind, status, used, linked, importance, source): (String, String, i64, String, f64, String) =
        store
            .conn()
            .query_row(
                "SELECT frag_kind, status, used_count, linked, importance, source
                   FROM fragments WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .unwrap();
    assert_eq!(kind, "question");
    assert_eq!(status, "pending", "新建的卡从「待问」开始");
    assert_eq!(used, 0, "还没问过，新颖度的账是 0");
    assert_eq!(linked, "[]", "关联列表与别的碎片同款（空 JSON 数组）");
    assert_eq!(importance, 0.8);
    assert_eq!(source, "core");

    let card = store.question_card(id).unwrap();
    assert_eq!(card.template_key, "foreshadow.due");
    assert_eq!(store.question_cards(work.id, None).unwrap().len(), 1);
    assert_eq!(
        store.question_cards(work.id, Some(QuestionState::Pending)).unwrap().len(),
        1
    );
    assert!(store.question_cards(work.id, Some(QuestionState::Asked)).unwrap().is_empty());

    // 建卡也留痕（来源 / 模板 / 溯源都进 payload，事后能追）
    let (op, payload) = op_rows(&store, id).pop().unwrap();
    assert_eq!(op, "create");
    assert!(payload.contains("foreshadow.due") && payload.contains("\"to\":\"pending\""));
}

#[test]
fn derived_card_points_back_at_the_question_that_sparked_it() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "题本").unwrap();
    let question = store.create_question_card(&new_card(work.id, "要不要给他一个转折点？")).unwrap();
    let idea = store
        .create_question_card(&NewQuestionCard {
            derived_from: Some(question),
            ..new_card(work.id, "让他把那封信烧了")
        })
        .unwrap();

    let card = store.question_card(idea).unwrap();
    assert_eq!(card.derived_from, Some(question), "灵感卡要指回勾出它的那张问题卡");
    // 跨书溯源要被拒：两本书的因果不能混在一起
    let other = store.create_work(WorkKind::Article, "另一本").unwrap();
    let err = store
        .create_question_card(&NewQuestionCard {
            derived_from: Some(question),
            ..new_card(other.id, "跨书派生")
        })
        .unwrap_err();
    assert_eq!(err.code(), "card.derived_from_invalid");
}

#[test]
fn creation_refuses_what_cannot_be_used() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "题本").unwrap();

    assert_eq!(
        store.create_question_card(&new_card(work.id, "   ")).unwrap_err().code(),
        "card.body_empty"
    );
    let err = store
        .create_question_card(&NewQuestionCard { importance: 1.5, ..new_card(work.id, "越界") })
        .unwrap_err();
    assert_eq!(err.code(), "card.importance_out_of_range");
    let err = store
        .create_question_card(&NewQuestionCard {
            derived_from: Some(999),
            ..new_card(work.id, "指向不存在的卡")
        })
        .unwrap_err();
    assert_eq!(err.code(), "card.derived_from_invalid");
    // 往已经删掉的书里建卡：说清是"作品没了"，不是一句数据库错误
    store.soft_delete_work(work.id).unwrap();
    let err = store.create_question_card(&new_card(work.id, "写进回收站里的书")).unwrap_err();
    assert_eq!(err.code(), "work.gone");
}

#[test]
fn every_transition_is_reachable_and_leaves_evidence() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "状态机").unwrap();

    for edge in TRANSITIONS {
        let id = store
            .create_question_card(&new_card(work.id, "被遍历的一条边"))
            .unwrap();
        drive_to(&mut store, id, edge.from);

        let before = op_rows(&store, id).len();
        let from = store.move_question_card(id, edge.to, "test:audit").unwrap();
        assert_eq!(from, edge.from, "返回的该是迁移前的状态");

        // 证据一：字段（谁落在哪张表哪个字段）
        assert_eq!(
            status_in_db(&store, id),
            edge.to.as_str(),
            "迁移 {} → {} 没落到 fragments.status",
            edge.from.as_str(),
            edge.to.as_str()
        );
        // 证据二：事件（谁触发、从哪到哪）
        let mut rows = op_rows(&store, id);
        assert_eq!(rows.len(), before + 1, "每次迁移只该留一条痕");
        let (op, payload) = rows.pop().unwrap();
        assert_eq!(op, edge.action, "op 该是动作码");
        for needle in [
            format!("\"from\":\"{}\"", edge.from.as_str()),
            format!("\"to\":\"{}\"", edge.to.as_str()),
            "\"trigger\":\"test:audit\"".to_string(),
        ] {
            assert!(payload.contains(&needle), "证据里缺 {needle}：{payload}");
        }

        // 迁移之后的状态与卡读出来的一致
        assert_eq!(store.question_card(id).unwrap().state, edge.to);
    }
}

#[test]
fn state_machine_has_no_dead_end_and_no_unreachable_state() {
    // 进得来：每个态都能从 pending 走到（path_to 走不到会 panic）
    for state in QuestionState::ALL {
        let _ = path_to(state);
    }
    // 出得去：只有声明为终态的态可以没有出边；终态也真的没有出边
    for state in QuestionState::ALL {
        let outgoing = TRANSITIONS.iter().filter(|t| t.from == state).count();
        assert_eq!(
            outgoing == 0,
            state.is_terminal(),
            "{}：出边 {outgoing} 条，终态声明={}——两者必须互补",
            state.as_str(),
            state.is_terminal()
        );
    }
    // 每条边都被上面那一条测试真的走过；这里再钉一次动作码不重复、不自环
    for (i, edge) in TRANSITIONS.iter().enumerate() {
        assert_ne!(edge.from, edge.to, "不许自环：{}", edge.action);
        for other in &TRANSITIONS[i + 1..] {
            assert!(
                !(other.from == edge.from && other.to == edge.to),
                "重复的边：{} → {}",
                edge.from.as_str(),
                edge.to.as_str()
            );
        }
    }
}

#[test]
fn asking_counts_toward_novelty_cooling() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "题本").unwrap();
    let id = store.create_question_card(&new_card(work.id, "问过一次的卡")).unwrap();

    store.move_question_card(id, QuestionState::Asked, "push").unwrap();
    assert_eq!(store.question_card(id).unwrap().used_count, 1, "问出一次，账上加一");

    // 舍弃不动这个账（它记的是"被问过几次"，不是"处置过几次"）
    store.move_question_card(id, QuestionState::Discarded, "author").unwrap();
    assert_eq!(store.question_card(id).unwrap().used_count, 1);

    // 捞回再问一次 → 2（新颖度冷却就靠这个数一路涨）
    store.move_question_card(id, QuestionState::Pending, "author").unwrap();
    store.move_question_card(id, QuestionState::Asked, "pull").unwrap();
    assert_eq!(store.question_card(id).unwrap().used_count, 2);

    // 走完整条延迟重出的路：asked → deferred → pending → asked
    store.move_question_card(id, QuestionState::Deferred, "author").unwrap();
    store.move_question_card(id, QuestionState::Pending, "requeue").unwrap();
    store.move_question_card(id, QuestionState::Asked, "push").unwrap();
    assert_eq!(store.question_card(id).unwrap().used_count, 3);
}

#[test]
fn illegal_moves_are_refused_and_write_nothing() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "题本").unwrap();
    let id = store.create_question_card(&new_card(work.id, "答完就收工")).unwrap();
    drive_to(&mut store, id, QuestionState::Answered);

    let before = op_rows(&store, id).len();
    // answered 是终态：没有出边（想再问是作者自己写新卡）
    let err = store.move_question_card(id, QuestionState::Asked, "author").unwrap_err();
    assert_eq!(err.code(), "card.illegal_transition");
    // 也不能跳过一个态（pending 不能直接跳到 answered）
    let fresh_id = store.create_question_card(&new_card(work.id, "不许跳态")).unwrap();
    let err = store
        .move_question_card(fresh_id, QuestionState::Answered, "author")
        .unwrap_err();
    assert_eq!(err.code(), "card.illegal_transition");

    assert_eq!(store.question_card(id).unwrap().state, QuestionState::Answered);
    assert_eq!(status_in_db(&store, id), "answered");
    assert_eq!(op_rows(&store, id).len(), before, "被拒的迁移不许留痕（更不许改状态）");
    assert_eq!(status_in_db(&store, fresh_id), "pending");
}

#[test]
fn missing_cards_and_soft_deleted_ones_are_not_cards() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "题本").unwrap();
    let id = store.create_question_card(&new_card(work.id, "会被挪进回收站的卡")).unwrap();

    assert_eq!(store.question_card(999).unwrap_err().code(), "card.not_found");
    assert_eq!(
        store.move_question_card(999, QuestionState::Asked, "push").unwrap_err().code(),
        "card.not_found"
    );

    // 软删的碎片不算卡（回收站语义与作品/节点一致：删了先不出现）
    store
        .conn()
        .execute("UPDATE fragments SET deleted_at = 1 WHERE id = ?1", [id])
        .unwrap();
    assert_eq!(store.question_card(id).unwrap_err().code(), "card.not_found");
    assert!(store.question_cards(work.id, None).unwrap().is_empty());
}

/// v8 只补列与索引，所以把版本号退回 v7 再打开，**卡与它的模板键一个都不许变**
/// （真机上"升级到一半断电/被杀"就长这样：版本号没推进，下次启动重跑）。
#[test]
fn rerunning_v8_keeps_cards() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "题本").unwrap();
    let id = store
        .create_question_card(&NewQuestionCard {
            template_key: "rhythm.same_tone".to_string(),
            ..new_card(work.id, "连续三章一个调子，要不要插个喘息？")
        })
        .unwrap();
    let path = std::path::PathBuf::from(store.conn().path().unwrap());
    drop(store);

    {
        let conn = yanmo_core::db::open(&path).unwrap();
        conn.pragma_update(None, "user_version", 7).unwrap();
    }
    let store = Store::open(&path).unwrap();
    let card = store.question_card(id).unwrap();
    assert_eq!(card.template_key, "rhythm.same_tone", "重跑 v8 不该弄丢模板键");
    assert_eq!(card.state, QuestionState::Pending);
    assert_eq!(
        yanmo_core::db::migrations::user_version(store.conn()).unwrap(),
        yanmo_core::db::migrations::schema_version(),
        "重跑之后版本号要回到最新"
    );
}
