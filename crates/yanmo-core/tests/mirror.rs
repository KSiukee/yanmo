//! 磁盘 `.md` 强镜像验收：**结构稳定、幂等、改名是 rename、别人的东西一个字不碰**。
//!
//! 这些断言盯的是"承诺能不能兑现"那一层：作者拿记事本改了稿子，研墨绝不许把它盖掉；
//! 章改了名，磁盘上该是同一个文件换了名字，而不是删一份再写一份（那在 git 里就是一次
//! "删了又加"，历史断掉）。路径与命名口径与导出**共用一套**（`store::tree_path`），
//! 所以这里也顺带把"两处产物会不会各走各的"钉住。

use std::collections::HashSet;

use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::{
    plan_mirror, ConflictReason, DiskState, MirrorAction, MirrorEntry, MirrorFile, Store,
};

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 一本两卷三章的书。
fn novel(store: &mut Store) -> i64 {
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let one = store.list_nodes(work.id).unwrap()[0].id;
    store.rename_node(one, "第一卷").unwrap(); // 根卷默认无名，起个名免得路径断言被那件事带着走
    let two = store.create_node(work.id, None, NodeKind::Volume, "第二卷").unwrap();
    for (parent, title, body) in [
        (one, "第一章", "第一章的正文。"),
        (one, "第二章", "第二章的正文。"),
        (two, "第三章", "第三章的正文。"),
    ] {
        let id = store.create_node(work.id, Some(parent), NodeKind::Chapter, title).unwrap();
        store.write_body(id, body).unwrap();
    }
    work.id
}

/// 假装磁盘：账上记过的路径都在、都是我们写的那份；`foreign` 里的改成"别人动过"。
fn disk<'a>(state: &'a [MirrorEntry], foreign: &'a [&'a str]) -> impl Fn(&str) -> DiskState + 'a {
    move |path: &str| {
        if foreign.contains(&path) {
            DiskState::Foreign
        } else if state.iter().any(|entry| entry.relative_path == path) {
            DiskState::Ours
        } else {
            DiskState::Missing
        }
    }
}

fn paths_of(files: &[MirrorFile]) -> Vec<String> {
    files.iter().map(|file| file.relative_path.clone()).collect()
}

fn text_of<'a>(files: &'a [MirrorFile], path: &str) -> &'a str {
    files
        .iter()
        .find(|file| file.relative_path == path)
        .map(|file| std::str::from_utf8(&file.content).unwrap())
        .unwrap_or_else(|| panic!("没有这个文件：{path}；实际有 {:?}", paths_of(files)))
}

/// 走一遍"渲染 → 对账 → 记账"，模拟壳里对好账之后的落定。
fn settle(store: &mut Store, work: i64, foreign: &[&str]) -> yanmo_core::store::MirrorPlan {
    let desired = store.render_mirror(work).unwrap();
    let state = store.mirror_state(work).unwrap();
    let plan = plan_mirror(&desired, &state, disk(&state, foreign), true);
    store.mirror_record(work, &plan.records).unwrap();
    plan
}

#[test]
fn the_mirror_keeps_the_same_path_language_as_the_export() {
    let (_dir, mut store) = fresh();
    let work = novel(&mut store);

    let files = store.render_mirror(work).unwrap();
    assert_eq!(
        paths_of(&files),
        vec![
            format!("长夜-{work}/001-第一卷/001-第一章.md"),
            format!("长夜-{work}/001-第一卷/002-第二章.md"),
            format!("长夜-{work}/002-第二卷/001-第三章.md"),
        ],
        "书名带 id（重名作品不许互相踩）、卷是目录、章是文件、序号保证顺序"
    );
    assert_eq!(
        text_of(&files, &format!("长夜-{work}/001-第一卷/001-第一章.md")),
        "# 第一章\n\n第一章的正文。\n",
        "文件自带标题行：单独拎一份出来也知道这是哪一章"
    );
}

#[test]
fn rendering_twice_gives_byte_identical_files() {
    let (_dir, mut store) = fresh();
    let work = novel(&mut store);
    assert_eq!(
        store.render_mirror(work).unwrap(),
        store.render_mirror(work).unwrap(),
        "同一份库内容渲染两次必须一模一样（否则每次对账都在制造 git diff）"
    );
}

#[test]
fn the_first_pass_writes_everything_and_the_second_writes_nothing() {
    let (_dir, mut store) = fresh();
    let work = novel(&mut store);

    let desired = store.render_mirror(work).unwrap();
    let plan = plan_mirror(&desired, &[], disk(&[], &[]), true);
    assert_eq!(plan.actions.len(), desired.len(), "第一次：每一份都要写出去");
    assert!(plan.conflicts.is_empty());
    store.mirror_record(work, &plan.records).unwrap();

    // 第二次：库没变、磁盘也没被别人动过 → 一个动作都没有
    let plan = settle(&mut store, work, &[]);
    assert!(plan.actions.is_empty(), "幂等：再对一次账不该有任何写盘动作：{:?}", plan.actions);
    assert_eq!(plan.unchanged, desired.len());
}

#[test]
fn renaming_a_chapter_renames_its_file_instead_of_deleting_and_rewriting() {
    let (_dir, mut store) = fresh();
    let work = novel(&mut store);
    settle(&mut store, work, &[]);

    let chapter = store
        .list_nodes(work)
        .unwrap()
        .into_iter()
        .find(|node| node.title_rendered == "第二章")
        .unwrap()
        .id;
    store.rename_node(chapter, "第二章 灯").unwrap();

    let desired = store.render_mirror(work).unwrap();
    let state = store.mirror_state(work).unwrap();
    let plan = plan_mirror(&desired, &state, disk(&state, &[]), true);
    let renames: Vec<&MirrorAction> = plan
        .actions
        .iter()
        .filter(|action| matches!(action, MirrorAction::Rename { .. }))
        .collect();
    assert_eq!(renames.len(), 1, "改名就是一次 rename：{:?}", plan.actions);
    let MirrorAction::Rename { from, to, .. } = renames[0] else { unreachable!() };
    assert!(from.ends_with("002-第二章.md"), "旧路径：{from}");
    assert!(to.ends_with("002-第二章 灯.md"), "新路径：{to}");
    assert!(
        !plan.actions.iter().any(|action| matches!(action, MirrorAction::Remove { .. })),
        "rename 之后不该再收一遍旧文件（那不是收残留，那是把刚挪过去的又删了）：{:?}",
        plan.actions
    );
}

#[test]
fn an_edit_outside_the_app_is_reported_and_never_overwritten() {
    let (_dir, mut store) = fresh();
    let work = novel(&mut store);
    settle(&mut store, work, &[]);

    let edited = store.mirror_state(work).unwrap()[0].relative_path.clone();
    store.write_body(store.list_nodes(work).unwrap()[0].id, "在研墨里改过一遍。").unwrap();

    let desired = store.render_mirror(work).unwrap();
    let state = store.mirror_state(work).unwrap();
    let plan = plan_mirror(&desired, &state, disk(&state, &[edited.as_str()]), true);

    assert!(
        !plan
            .actions
            .iter()
            .any(|action| matches!(action, MirrorAction::Write { relative_path, .. } if relative_path == &edited)),
        "作者用记事本改过的那一份，一个字都不许盖：{:?}",
        plan.actions
    );
    assert!(
        plan.conflicts
            .iter()
            .any(|c| c.relative_path == edited && c.reason == ConflictReason::EditedExternally),
        "但必须如实报出来，绝不静默丢弃：{:?}",
        plan.conflicts
    );

    // 记账之后：这份冲突不再算"没写完"（否则每两秒都要把整本书的正文重扫一遍）
    store.mirror_record(work, &plan.records).unwrap();
    assert!(
        !store.mirror_pending(work).unwrap(),
        "冲突行本身不该让这本书一直待办（否则每两秒都要把整本书的正文重扫一遍）"
    );
}

#[test]
fn a_file_we_did_not_write_is_never_overwritten() {
    let (_dir, mut store) = fresh();
    let work = novel(&mut store);
    let desired = store.render_mirror(work).unwrap();
    let occupied = desired[0].relative_path.clone();

    // 账是空的、路径上却有一份文件：那是别人的东西
    let plan = plan_mirror(&desired, &[], disk(&[], &[occupied.as_str()]), true);
    assert!(
        !plan
            .actions
            .iter()
            .any(|action| matches!(action, MirrorAction::Write { relative_path, .. } if relative_path == &occupied)),
        "路径上不是我们写的东西，不许覆盖：{:?}",
        plan.actions
    );
    assert!(plan
        .conflicts
        .iter()
        .any(|c| c.relative_path == occupied && c.reason == ConflictReason::ForeignFile));
    // 别的文件照写不误：一处挡路不该让整本书镜像都停摆
    assert!(plan.actions.len() >= desired.len() - 1);
}

#[test]
fn deleting_a_chapter_collects_its_file_without_touching_the_others() {
    let (_dir, mut store) = fresh();
    let work = novel(&mut store);
    settle(&mut store, work, &[]);

    let chapter = store
        .list_nodes(work)
        .unwrap()
        .into_iter()
        .find(|node| node.title_rendered == "第二章")
        .unwrap()
        .id;
    store.soft_delete_node(chapter).unwrap();

    let desired = store.render_mirror(work).unwrap();
    let state = store.mirror_state(work).unwrap();
    let plan = plan_mirror(&desired, &state, disk(&state, &[]), true);
    assert_eq!(plan.actions.len(), 1, "只该收那一份：{:?}", plan.actions);
    let MirrorAction::Remove { relative_path, .. } = &plan.actions[0] else {
        panic!("应当是 Remove：{:?}", plan.actions);
    };
    assert!(relative_path.ends_with("002-第二章.md"));
    assert!(plan.dropped.contains(&chapter), "账上那一行也要跟着去掉");
}

/// 收残留**必须避开这一轮要写 / 要改名过去的位置**——真踩过的那类事故：
/// 一个节点挪进了别人刚腾出来的路径，残留清理又把那份新写的当孤儿删了。
#[test]
fn collecting_leftovers_never_deletes_a_path_this_pass_just_filled() {
    let file = |node_id: i64, path: &str| MirrorFile {
        node_id,
        relative_path: path.to_string(),
        body_hash: format!("body-{node_id}"),
        content: format!("content-{node_id}").into_bytes(),
        file_hash: format!("file-{node_id}"),
    };
    let entry = |node_id: i64, path: &str| MirrorEntry {
        node_id,
        relative_path: path.to_string(),
        body_hash: format!("body-{node_id}"),
        file_hash: format!("file-{node_id}"),
        size_bytes: 10,
        conflict: false,
    };

    // A 从 001-x 挪到 002-x；原本占着 002-x 的 B 挪到 003-x
    let desired = vec![file(1, "w/002-x.md"), file(2, "w/003-x.md")];
    let previous = vec![entry(1, "w/001-x.md"), entry(2, "w/002-x.md")];
    let plan = plan_mirror(&desired, &previous, disk(&previous, &[]), true);

    let order: Vec<&str> = plan
        .actions
        .iter()
        .map(|action| match action {
            MirrorAction::Rename { .. } => "rename",
            MirrorAction::Write { .. } => "write",
            MirrorAction::Remove { .. } => "remove",
        })
        .collect();
    assert_eq!(order, vec!["rename", "write", "remove"], "先腾位置、再写、最后才收：{order:?}");
    let filled: HashSet<&str> = plan
        .actions
        .iter()
        .filter_map(|action| match action {
            MirrorAction::Write { relative_path, .. } => Some(relative_path.as_str()),
            MirrorAction::Rename { to, .. } => Some(to.as_str()),
            _ => None,
        })
        .collect();
    for action in &plan.actions {
        if let MirrorAction::Remove { relative_path, .. } = action {
            assert!(
                !filled.contains(relative_path.as_str()),
                "收残留删到了这一轮刚填上的位置：{relative_path}"
            );
        }
    }
    assert_eq!(plan.records.len(), 2);
}

#[test]
fn a_body_change_makes_the_mirror_pending_and_recording_settles_it() {
    let (_dir, mut store) = fresh();
    let work = novel(&mut store);
    settle(&mut store, work, &[]);
    assert!(!store.mirror_pending(work).unwrap(), "刚对完账应当是齐的");

    let chapter = store
        .list_nodes(work)
        .unwrap()
        .into_iter()
        .find(|node| node.title_rendered == "第一章")
        .unwrap()
        .id;
    store.write_body(chapter, "又写了一段。").unwrap();
    assert!(store.mirror_pending(work).unwrap(), "正文变了就是待办（不读正文也该知道）");

    settle(&mut store, work, &[]);
    assert!(!store.mirror_pending(work).unwrap(), "对完账就该收工");
}

#[test]
fn two_books_with_the_same_title_get_two_directories() {
    let (_dir, mut store) = fresh();
    let first = store.create_work(WorkKind::Novel, "同名").unwrap();
    let second = store.create_work(WorkKind::Novel, "同名").unwrap();
    let one = store.list_nodes(first.id).unwrap()[0].id;
    let two = store.list_nodes(second.id).unwrap()[0].id;
    let a = store.create_node(first.id, Some(one), NodeKind::Chapter, "章").unwrap();
    let b = store.create_node(second.id, Some(two), NodeKind::Chapter, "章").unwrap();
    store.write_body(a, "甲的正文。").unwrap();
    store.write_body(b, "乙的正文。").unwrap();

    let first_paths = paths_of(&store.render_mirror(first.id).unwrap());
    let second_paths = paths_of(&store.render_mirror(second.id).unwrap());
    assert_ne!(first_paths, second_paths, "同名作品必须各有各的目录");
    assert!(first_paths[0].starts_with(&format!("同名-{}/", first.id)));
    assert!(second_paths[0].starts_with(&format!("同名-{}/", second.id)));
}

/// 书进了回收站（软删）之后，账还在、书已经不在了——对账要能按账把文件收干净。
#[test]
fn a_recoded_work_still_shows_up_so_its_files_can_be_collected() {
    let (_dir, mut store) = fresh();
    let work = novel(&mut store);
    settle(&mut store, work, &[]);
    store.soft_delete_work(work).unwrap();

    assert!(store.mirror_works().unwrap().contains(&work), "账还在，就得照看它");
    assert!(store.render_mirror(work).unwrap().is_empty(), "已删的书没有该有的文件");

    let state = store.mirror_state(work).unwrap();
    let plan = plan_mirror(&[], &state, disk(&state, &[]), true);
    assert_eq!(plan.actions.len(), state.len(), "每一份都收掉：{:?}", plan.actions);
    assert!(plan.records.is_empty() && plan.dropped.len() == state.len());
}

#[test]
fn the_switch_defaults_to_on_and_can_be_turned_off() {
    let (_dir, mut store) = fresh();
    assert!(store.mirror_enabled().unwrap(), "默认必须是开着的——这是硬承诺");
    store.set_mirror_enabled(false).unwrap();
    assert!(!store.mirror_enabled().unwrap(), "作者关得掉");
    store.set_mirror_enabled(true).unwrap();
    assert!(store.mirror_enabled().unwrap(), "也重开得回来");
}

#[test]
fn an_empty_chapter_still_gets_a_file_with_its_title() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Article, "还没写的短篇").unwrap();
    let piece = store.list_nodes(work.id).unwrap()[0].id;
    store.rename_node(piece, "开头").unwrap();

    let files = store.render_mirror(work.id).unwrap();
    assert_eq!(paths_of(&files), vec![format!("还没写的短篇-{}/001-开头.md", work.id)]);
    assert_eq!(text_of(&files, &files[0].relative_path), "# 开头\n");
}

/// 日常对账**不逐份比磁盘**（性能取舍），全量核对才比。
///
/// 这条差异是明写的：作者每敲一下都要把整本书的 `.md` 重读一遍去比字节，那点 I/O
/// 迟早会吃掉"边写边存"的顺滑。所以"文件被人删了"这类事由启动 / 空闲巡检 / 手动同步兜。
#[test]
fn a_daily_pass_trusts_the_account_while_a_full_pass_checks_the_disk() {
    let (_dir, mut store) = fresh();
    let work = novel(&mut store);
    settle(&mut store, work, &[]);

    let desired = store.render_mirror(work).unwrap();
    let state = store.mirror_state(work).unwrap();
    let vanished = state[0].relative_path.clone();
    let gone = |path: &str| {
        if path == vanished {
            DiskState::Missing
        } else {
            DiskState::Ours
        }
    };

    let daily = plan_mirror(&desired, &state, gone, false);
    assert!(daily.actions.is_empty(), "日常对账信账、不看磁盘：{:?}", daily.actions);

    let full = plan_mirror(&desired, &state, gone, true);
    assert_eq!(full.actions.len(), 1, "全量核对才发现那份文件没了：{:?}", full.actions);
    assert!(
        matches!(&full.actions[0], MirrorAction::Write { relative_path, .. } if relative_path == &vanished)
    );
}

/// 作者把文件改回来之后，冲突要**自己解除**（而不是永远挂在那儿）。
///
/// 这一条是"冲突不是判死刑"的兑现：账上挂着"被人改过"只是因为磁盘上不是我们写的那份；
/// 一旦那份又变回我们要的内容，就没有什么可定夺的了——账当场清干净，也不必多写一次盘。
#[test]
fn a_conflict_clears_itself_once_the_file_is_back_to_our_version() {
    let (_dir, mut store) = fresh();
    let work = novel(&mut store);
    settle(&mut store, work, &[]);

    // 先制造一次"外面改过"
    let edited = store.mirror_state(work).unwrap()[0].relative_path.clone();
    let desired = store.render_mirror(work).unwrap();
    let state = store.mirror_state(work).unwrap();
    let plan = plan_mirror(&desired, &state, disk(&state, &[edited.as_str()]), true);
    assert!(plan.conflicts.iter().any(|c| c.relative_path == edited));
    store.mirror_record(work, &plan.records).unwrap();
    assert!(store.mirror_state(work).unwrap().iter().any(|entry| entry.conflict));

    // 作者把它改回来了（磁盘上那份又跟我们写的一模一样）
    let state = store.mirror_state(work).unwrap();
    let plan = plan_mirror(&desired, &state, disk(&state, &[]), true);
    assert!(plan.actions.is_empty(), "改回来就不必再写一遍：{:?}", plan.actions);
    assert!(plan.conflicts.is_empty(), "冲突该当场解除：{:?}", plan.conflicts);
    store.mirror_record(work, &plan.records).unwrap();
    assert!(store.mirror_state(work).unwrap().iter().all(|entry| !entry.conflict));
}
