//! SOP 验收：**跟人、按作品可覆盖、核心零文案、留痕可回滚**。
//!
//! 判据（对着这一条立的那份验收写）：
//! 1. 没设过 → 内置那套起手流程（**只有 id 与顺序**，一个字的文案都没有）；
//! 2. 全局设过 → 用它；书覆盖 → **整份覆盖**（不是字段级合并）；
//! 3. 层内只存改过的项（`None` = 用字典默认文案），清空 = 把这层记录删掉；
//! 4. 写错了当场报（码 + 取值），读回来那条路宽（坏 JSON / 坏项当没设过）；
//! 5. 每次改动与它**同一个事务**留一条 op-log（payload 是整份），可查历史、可回滚，
//!    而回滚本身又是一次新改动——**历史不涂改**；
//! 6. 铁律守卫：**没有"下一步"门禁、没有打卡/连续天数**——序列化出来的键是白名单。

use yanmo_core::error::codes;
use yanmo_core::model::{SideTab, WorkKind};
use yanmo_core::store::{Sop, SopAction, SopCheck, SopSource, SopStep, Store};

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 库里某个键的原文（没这个键就是 None）。
fn raw(store: &Store, key: &str) -> Option<String> {
    store
        .conn()
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
        .ok()
}

/// op-log 里的 SOP 改动（新的在前）。
fn log_rows(store: &Store, work_id: Option<i64>) -> Vec<(i64, String, String)> {
    let mut stmt = store
        .conn()
        .prepare(
            "SELECT seq, op, payload FROM op_log
              WHERE entity = 'settings' AND entity_id = ?1 ORDER BY seq",
        )
        .unwrap();
    let rows = stmt
        .query_map([work_id.unwrap_or(0)], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .unwrap();
    rows.map(|row| row.unwrap()).collect()
}

/// 一条两步骤的 SOP（第一步带名字、参考时长、检查项与绑定动作）。
fn sample() -> Sop {
    Sop {
        steps: vec![
            SopStep {
                id: "core".into(),
                name: Some("先想清楚要写什么".into()),
                note: Some("一句话说得出才算想清楚".into()),
                minutes: Some(20),
                checks: vec![
                    SopCheck { id: "one-line".into(), text: Some("能用一句话说清".into()), done: true },
                    SopCheck { id: "who".into(), text: None, done: false },
                ],
                action: Some(SopAction { target: "aside".into(), value: "creator".into() }),
                skipped: false,
            },
            SopStep { id: "draft".into(), skipped: true, ..Default::default() },
        ],
    }
}

#[test]
fn nothing_set_falls_back_to_the_builtin_starter_flow() {
    let (_dir, mut store) = fresh();
    let sop = store.sop(None).unwrap();
    assert_eq!(sop.source, SopSource::Default);
    let ids: Vec<&str> = sop.steps.iter().map(|step| step.id.as_str()).collect();
    assert_eq!(ids, ["core", "outline", "draft", "revise"], "内置那套只给 id 与顺序");
    // 核心零文案：一个字的默认文案都不该从核心里出来
    assert!(
        sop.steps.iter().all(|step| step.name.is_none() && step.note.is_none()),
        "默认步骤的名字必须留给界面字典"
    );
    assert!(raw(&store, "sop").is_none(), "读默认不该顺手写库");
    // 书的覆盖也没有：这本书照样用内置那套
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    assert_eq!(store.sop(Some(work.id)).unwrap().source, SopSource::Default);
}

#[test]
fn the_builtin_step_ids_are_dictionary_safe() {
    // 它们要拼成界面字典键 `sop.step.<id>`：点号 / 空格 / 引号会把字典查崩，而且是静默的
    for id in yanmo_core::store::DEFAULT_STEP_IDS {
        assert!(
            !id.is_empty()
                && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "内置 id 不合规：{id}"
        );
    }
}

#[test]
fn writes_stay_sparse_and_only_changed_items_land() {
    let (_dir, mut store) = fresh();
    store.set_sop(None, sample()).unwrap();

    let stored = raw(&store, "sop").expect("改过就该有记录");
    assert!(stored.contains("core"), "{stored}");
    assert!(!stored.contains("null"), "只存改过的项，不写空档：{stored}");
    assert!(!stored.contains("\"skipped\":false"), "false 也是默认值，不该写进库：{stored}");

    let back = store.sop(None).unwrap();
    assert_eq!(back.source, SopSource::Global);
    assert_eq!(back.steps[0].name.as_deref(), Some("先想清楚要写什么"));
    assert_eq!(back.steps[0].minutes, Some(20));
    assert_eq!(back.steps[0].checks.len(), 2);
    assert!(back.steps[0].checks[0].done, "勾过的要读得回来");
    assert_eq!(back.steps[0].checks[1].text, None, "没改过的检查项文案仍是空档");
    assert!(back.steps[1].skipped, "跳过标记要读得回来");

    // 空 SOP = 把这层记录删掉（回到继承 / 内置）
    store.set_sop(None, Sop::default()).unwrap();
    assert!(raw(&store, "sop").is_none(), "清空就该把键删掉");
    assert_eq!(store.sop(None).unwrap().source, SopSource::Default);
}

#[test]
fn a_work_override_replaces_the_whole_flow_not_step_by_step() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    // 全局：内置那套加一步"回头改"；这本书：只有两步的临时流程
    store.set_sop(None, sample()).unwrap();
    store
        .set_sop(
            Some(work.id),
            Sop {
                steps: vec![
                    SopStep { id: "quick".into(), name: Some("这一篇快点写".into()), ..Default::default() },
                    SopStep { id: "ship".into(), ..Default::default() },
                ],
            },
        )
        .unwrap();

    let mine = store.sop(Some(work.id)).unwrap();
    assert_eq!(mine.source, SopSource::Work, "界面要说得出「这份只属于这一篇」");
    let ids: Vec<&str> = mine.steps.iter().map(|step| step.id.as_str()).collect();
    assert_eq!(ids, ["quick", "ship"], "整份覆盖：全局那两步一步都不该混进来");

    let global = store.sop(None).unwrap();
    assert_eq!(global.source, SopSource::Global);
    assert_eq!(global.steps.len(), 2, "全局那份没被动过");
    assert!(raw(&store, &format!("work.{}.sop", work.id)).is_some(), "覆盖按书写键");

    // 另一本书没单独设过：用全局那份
    let other = store.create_work(WorkKind::Novel, "别的书").unwrap();
    assert_eq!(store.sop(Some(other.id)).unwrap().source, SopSource::Global);

    // 这本书回到继承
    store.reset_sop(Some(work.id)).unwrap();
    assert_eq!(store.sop(Some(work.id)).unwrap().source, SopSource::Global);
}

#[test]
fn bad_records_do_not_lock_the_panel() {
    let (_dir, store) = fresh();
    store
        .conn()
        .execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES('sop', '这不是 JSON', 0)",
            [],
        )
        .unwrap();
    assert_eq!(store.sop(None).unwrap().source, SopSource::Default, "坏 JSON 当没设过");

    // 坏项丢坏项：认不出的动作、越界时长、重复 id、空 id —— 好的那些照常读出来
    store
        .conn()
        .execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES('sop', ?1, 0)",
            [r#"{"steps":[
                {"id":"a","minutes":99999,"action":{"target":"aside","value":"timeline"}},
                {"id":"a"},
                {"id":"  "},
                {"id":"b","checks":[{"id":"x","text":"  收边  "},{"id":"x"},{"id":""}]}
            ]}"#],
        )
        .unwrap();
    let sop = store.sop(None).unwrap();
    let ids: Vec<&str> = sop.steps.iter().map(|step| step.id.as_str()).collect();
    assert_eq!(ids, ["a", "b"], "重复与空 id 丢掉，好的留着：{ids:?}");
    assert_eq!(sop.steps[0].minutes, None, "越界时长当没设");
    assert_eq!(sop.steps[0].action, None, "认不出的面板码把动作整个丢掉");
    assert_eq!(sop.steps[1].checks.len(), 1);
    assert_eq!(sop.steps[1].checks[0].text.as_deref(), Some("收边"), "文字收边");
}

#[test]
fn wrong_input_is_refused_with_its_own_code() {
    let (_dir, mut store) = fresh();
    let cases: Vec<(Sop, &str)> = vec![
        (
            Sop { steps: vec![SopStep { id: "  ".into(), ..Default::default() }] },
            codes::SOP_ID_EMPTY,
        ),
        (
            Sop { steps: vec![SopStep { id: "step.1".into(), ..Default::default() }] },
            codes::SOP_ID_BAD,
        ),
        (
            Sop {
                steps: vec![
                    SopStep { id: "a".into(), ..Default::default() },
                    SopStep { id: "a".into(), ..Default::default() },
                ],
            },
            codes::SOP_ID_DUPLICATE,
        ),
        (
            Sop {
                steps: vec![SopStep {
                    id: "a".into(),
                    checks: vec![
                        SopCheck { id: "c".into(), ..Default::default() },
                        SopCheck { id: "c".into(), ..Default::default() },
                    ],
                    ..Default::default()
                }],
            },
            codes::SOP_ID_DUPLICATE,
        ),
        (
            Sop {
                steps: vec![SopStep {
                    id: "a".into(),
                    name: Some("字".repeat(201)),
                    ..Default::default()
                }],
            },
            codes::SOP_TEXT_TOO_LONG,
        ),
        (
            Sop { steps: vec![SopStep { id: "a".into(), minutes: Some(0), ..Default::default() }] },
            codes::SOP_MINUTES_BAD,
        ),
        (
            Sop {
                steps: vec![SopStep {
                    id: "a".into(),
                    action: Some(SopAction { target: "question".into(), value: "warm".into() }),
                    ..Default::default()
                }],
            },
            codes::SOP_ACTION_UNKNOWN_TARGET,
        ),
        (
            Sop {
                steps: vec![SopStep {
                    id: "a".into(),
                    action: Some(SopAction { target: "aside".into(), value: "timeline".into() }),
                    ..Default::default()
                }],
            },
            codes::UNKNOWN_ASIDE_TAB,
        ),
    ];
    for (value, want) in cases {
        let error = store.set_sop(None, value).unwrap_err();
        assert_eq!(error.code(), want, "错的那一项要说得出自己的码");
    }
    assert!(raw(&store, "sop").is_none(), "一条都没写进去");
    assert!(log_rows(&store, None).is_empty(), "写失败不该留痕");

    // 太多了也拦（手滑粘进一份巨型清单的兜底）
    let many = Sop {
        steps: (0..yanmo_core::store::MAX_STEPS + 1)
            .map(|index| SopStep { id: format!("s{index}"), ..Default::default() })
            .collect(),
    };
    assert_eq!(store.set_sop(None, many).unwrap_err().code(), codes::SOP_TOO_MANY_STEPS);
}

#[test]
fn every_change_leaves_a_whole_copy_in_the_op_log() {
    let (_dir, mut store) = fresh();
    store.set_sop(None, sample()).unwrap();
    store.reset_sop(None).unwrap();

    let rows = log_rows(&store, None);
    assert_eq!(rows.len(), 2, "改两次留两条");
    assert_eq!(rows[0].1, "set_sop");
    assert_eq!(rows[1].1, "reset_sop");
    let payload: serde_json::Value = serde_json::from_str(&rows[0].2).unwrap();
    assert_eq!(payload["steps"][0]["id"], "core", "payload 是整份，不是补丁");
    assert_eq!(payload["steps"][0]["checks"][0]["done"], true, "差异可比");

    // 书那层与全局那层各记各的
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    store.set_sop(Some(work.id), sample()).unwrap();
    assert_eq!(log_rows(&store, Some(work.id)).len(), 1);
    assert_eq!(log_rows(&store, None).len(), 2, "全局那两条没被带累");
}

#[test]
fn history_reads_back_newest_first_and_rollback_keeps_the_trail() {
    let (_dir, mut store) = fresh();
    let first = sample();
    store.set_sop(None, first.clone()).unwrap();
    store
        .set_sop(
            None,
            Sop { steps: vec![SopStep { id: "only".into(), name: Some("简化成一步".into()), ..Default::default() }] },
        )
        .unwrap();

    let history = store.sop_history(None, 10).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].sop.steps[0].id, "only", "新的在前");
    assert_eq!(history[1].sop.steps[0].id, "core", "旧的那份一模一样地存着");
    assert!(history[1].sop.steps[0].checks[0].done, "整份都在（勾选状态也在）");
    let old_seq = history[1].seq;

    // 回滚到第一次那份：内容回来了，而历史变成三条（回滚本身也是一次改动）
    store.restore_sop(None, old_seq).unwrap();
    let now = store.sop(None).unwrap();
    assert_eq!(now.steps.len(), 2);
    assert_eq!(now.steps[0].name.as_deref(), Some("先想清楚要写什么"));
    assert_eq!(store.sop_history(None, 10).unwrap().len(), 3, "历史不涂改");

    // 找不到那条记录：报得出来，而且不乱盖
    let error = store.restore_sop(None, 999_999).unwrap_err();
    assert_eq!(error.code(), codes::SOP_REVISION_NOT_FOUND);

    // 另一本书的历史与它隔开：拿全局的 seq 去回滚这本书，也该说找不到
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let error = store.restore_sop(Some(work.id), old_seq).unwrap_err();
    assert_eq!(error.code(), codes::SOP_REVISION_NOT_FOUND, "别把别的层的流程盖上来");
}

#[test]
fn the_stored_shape_has_no_scoreboard_fields() {
    let (_dir, mut store) = fresh();
    store.set_sop(None, sample()).unwrap();
    let value: serde_json::Value =
        serde_json::from_str(&raw(&store, "sop").unwrap()).unwrap();

    // 白名单：只有"这一步是什么 + 勾没勾"。**没有连续天数、没有达成日期、没有打卡记录**
    // ——铁律写着"绝不做打卡/连续天数惩罚"，这条守卫让下一版想加也当场红。
    let step_keys: Vec<&str> = value["steps"][0].as_object().unwrap().keys().map(String::as_str).collect();
    for key in &step_keys {
        assert!(
            ["id", "name", "note", "minutes", "checks", "action", "skipped"].contains(key),
            "SOP 里冒出了白名单以外的字段：{key}（打卡/连续天数一律不许进库）"
        );
    }
    let check_keys: Vec<&str> = value["steps"][0]["checks"][0]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    for key in &check_keys {
        assert!(["id", "text", "done"].contains(key), "检查项多了不该有的字段：{key}");
    }

    // 顺序不设门禁：任何一步都能勾、能跳过，读回来的顺序就是作者排的顺序
    let mut sop = sample();
    sop.steps[1].checks.push(SopCheck { id: "free".into(), text: None, done: true });
    store.set_sop(None, sop).unwrap();
    let back = store.sop(None).unwrap();
    assert!(back.steps[1].checks[0].done, "第二步照样能勾");
    assert!(back.steps[1].skipped, "跳过与勾选互不干扰");

    // 绑定动作的码表就是界面那两片：认得出 flow / creator
    for tab in SideTab::ALL {
        let mut one = sample();
        one.steps[0].action = Some(SopAction { target: "aside".into(), value: tab.as_str().into() });
        store.set_sop(None, one).unwrap();
    }
}
