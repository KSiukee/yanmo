//! 章节导航与光标记忆：**"下一章"要跨卷**，光标要**认章节**。
//!
//! 这两条都是"只挂当前章"的前提：切章要切得对（顺序对、不串章），
//! 回来要回得准（光标是这一章的才用，别的章的光标宁可不要）。

use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::{EditorCursor, Store};

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

struct Book {
    store: Store,
    _dir: tempfile::TempDir,
    work_id: i64,
    chapters: Vec<i64>,
    volumes: Vec<i64>,
}

/// 造一本两卷四章的书：卷一[第一章, 第二章]、卷二[第三章, 第四章]。
fn two_volume_book() -> Book {
    let (dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume_one = store.list_nodes(work.id).unwrap()[0].id;
    let volume_two = store
        .create_node(work.id, None, NodeKind::Volume, "第二卷")
        .unwrap();
    let mut chapters = Vec::new();
    for (parent, title) in [
        (volume_one, "第一章"),
        (volume_one, "第二章"),
        (volume_two, "第三章"),
        (volume_two, "第四章"),
    ] {
        let id = store.create_node(work.id, Some(parent), NodeKind::Chapter, title).unwrap();
        store.write_body(id, &format!("{title}的正文。")).unwrap();
        chapters.push(id);
    }
    Book {
        store,
        _dir: dir,
        work_id: work.id,
        chapters,
        volumes: vec![volume_one, volume_two],
    }
}

#[test]
fn next_chapter_crosses_volume_boundaries() {
    let book = two_volume_book();
    let (one, two, three, four) = (
        book.chapters[0],
        book.chapters[1],
        book.chapters[2],
        book.chapters[3],
    );

    let first = book.store.chapter_neighbors(one).unwrap();
    assert_eq!(first.index, 1);
    assert_eq!(first.total, 4);
    assert_eq!(first.previous, None, "第一章前面没有东西");
    assert_eq!(first.next.as_ref().unwrap().id, two);

    // ★ 关键：第二章的下一章在**另一卷**里，只按同级排就会在这里断掉
    let second = book.store.chapter_neighbors(two).unwrap();
    assert_eq!(second.index, 2);
    assert_eq!(second.next.as_ref().unwrap().id, three, "下一章必须跨卷");
    assert_eq!(second.next.as_ref().unwrap().title, "第三章");

    let last = book.store.chapter_neighbors(four).unwrap();
    assert_eq!(last.index, 4);
    assert_eq!(last.next, None, "最后一章后面没有东西");
    assert_eq!(last.previous.as_ref().unwrap().id, three);
}

#[test]
fn containers_are_never_navigation_targets() {
    let book = two_volume_book();
    let volume = book.volumes[0];
    assert!(
        book.store.chapter_neighbors(volume).is_err(),
        "卷是容器不是章节，不能当导航目标"
    );
}

#[test]
fn deleted_chapters_drop_out_of_navigation() {
    let mut book = two_volume_book();
    let (one, two, three) = (book.chapters[0], book.chapters[1], book.chapters[2]);

    book.store.soft_delete_node(two).unwrap();
    let first = book.store.chapter_neighbors(one).unwrap();
    assert_eq!(first.total, 3);
    assert_eq!(first.next.as_ref().unwrap().id, three, "删掉的章节不该还挡在路上");
}

#[test]
fn scene_cards_participate_in_reading_order() {
    let mut book = two_volume_book();
    let chapter = book.chapters[0];
    let work_id = book.work_id;
    let scene = book
        .store
        .create_node(work_id, Some(chapter), NodeKind::Scene, "雨夜")
        .unwrap();
    book.store.write_body(scene, "场景卡的正文。").unwrap();

    let neighbors = book.store.chapter_neighbors(chapter).unwrap();
    assert_eq!(
        neighbors.next.as_ref().unwrap().id,
        scene,
        "场景卡也是写到正文的地方，应当按阅读顺序排进去（目录树任务会给出层级视图）"
    );
    assert_eq!(neighbors.total, 5);
}

#[test]
fn cursor_sticks_to_its_own_chapter() {
    let book = two_volume_book();
    let (one, two) = (book.chapters[0], book.chapters[1]);
    let cursor = EditorCursor {
        anchor: 42,
        head: 50,
        scroll_top: 1200,
    };

    assert_eq!(book.store.load_cursor(one).unwrap(), None, "没记过就没有");
    book.store.save_cursor(one, cursor).unwrap();
    assert_eq!(book.store.load_cursor(one).unwrap(), Some(cursor));
    assert_eq!(
        book.store.load_cursor(two).unwrap(),
        None,
        "别的章不能套用这一章的光标"
    );

    // 切到第二章：光标记录跟着走，第一章的记录就不该再被认
    let other = EditorCursor { anchor: 3, head: 3, scroll_top: 0 };
    book.store.save_cursor(two, other).unwrap();
    assert_eq!(book.store.load_cursor(two).unwrap(), Some(other));
    assert_eq!(book.store.load_cursor(one).unwrap(), None);
}

#[test]
fn cursor_survives_reopen_and_refuses_deleted_nodes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("yanmo.db");
    let node = {
        let mut store = Store::open(&path).unwrap();
        let work = store.create_work(WorkKind::Article, "随笔").unwrap();
        let node = store.list_nodes(work.id).unwrap()[0].id;
        store
            .save_cursor(node, EditorCursor { anchor: 9, head: 9, scroll_top: 300 })
            .unwrap();
        node
    };
    {
        let store = Store::open(&path).unwrap();
        assert_eq!(
            store.load_cursor(node).unwrap(),
            Some(EditorCursor { anchor: 9, head: 9, scroll_top: 300 }),
            "光标要活过重启（它就是给「回到原位」用的）"
        );
    }
    let mut store = Store::open(&path).unwrap();
    store.soft_delete_node(node).unwrap();
    assert!(store.save_cursor(node, EditorCursor { anchor: 1, head: 1, scroll_top: 0 }).is_err());
    assert_eq!(store.load_cursor(node).unwrap(), None);
}

#[test]
fn new_chapter_lands_right_after_the_current_one() {
    let mut book = two_volume_book();
    let (one, two) = (book.chapters[0], book.chapters[1]);

    let created = book
        .store
        .add_chapter_after(one, NodeKind::Chapter, "第一章·补")
        .unwrap();

    let after_one = book.store.chapter_neighbors(one).unwrap();
    assert_eq!(after_one.total, 5, "多了一章");
    assert_eq!(after_one.next.as_ref().unwrap().id, created, "新章就插在当前章后面");

    let created_view = book.store.chapter_neighbors(created).unwrap();
    assert_eq!(created_view.index, 2, "序号是密集的，插在第二位");
    assert_eq!(created_view.next.as_ref().unwrap().id, two, "原来的第二章往后顺延");

    // 插在末尾那一章后面时，应当留在同一卷里
    let last_in_volume = book.chapters[1];
    let tail = book
        .store
        .add_chapter_after(last_in_volume, NodeKind::Chapter, "第二章·补")
        .unwrap();
    let work_id = book.work_id;
    let nodes = book.store.list_nodes(work_id).unwrap();
    let parent_of = |id: i64| nodes.iter().find(|n| n.id == id).unwrap().parent_id;
    assert_eq!(parent_of(tail), parent_of(last_in_volume), "新章跟当前章同父（不跨卷乱跑）");
}

#[test]
fn default_chapter_name_counts_inside_its_own_volume() {
    let mut book = two_volume_book();
    let (one, volume_two) = (book.chapters[0], book.volumes[1]);
    let work_id = book.work_id;
    let title_of = |store: &Store, id: i64| {
        store
            .list_nodes(work_id)
            .unwrap()
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.title.clone())
            .unwrap()
    };

    // 标题留空 = 按**同层**取号：第一卷里已有两章，所以新章是第三
    // （按整本书数会变成第五——那就是"分卷之后跳号"的老毛病）
    let created = book.store.add_chapter_after(one, NodeKind::Chapter, "").unwrap();
    assert_eq!(title_of(&book.store, created), "第3章");

    // 另一卷各数各的：第二卷里已有的号是 3、4，所以这一层下一个是 5
    // （"各层各数"数的是**这一层用过的号**，不是"这一层有几章"）
    let in_second = book
        .store
        .create_node(book.work_id, Some(volume_two), NodeKind::Chapter, "")
        .unwrap();
    assert_eq!(title_of(&book.store, in_second), "第5章", "只数自己这一层的号");

    // 卷也一样按同层取号
    let volume_three = book.store.create_node(book.work_id, None, NodeKind::Volume, "").unwrap();
    assert_eq!(title_of(&book.store, volume_three), "第3卷", "根级已有两卷");
}

#[test]
fn ancestors_run_from_root_down_to_the_parent() {
    let mut book = two_volume_book();
    let chapter = book.chapters[2]; // 第二章在第二卷里

    // 给第二卷里的第三章再挂一张场景卡，试四层
    let scene = book
        .store
        .create_node(book.work_id, Some(chapter), NodeKind::Scene, "场景卡")
        .unwrap();

    let chain = book.store.node_ancestors(scene).unwrap();
    assert_eq!(
        chain,
        vec![book.volumes[1], chapter],
        "祖先链从根往下排，且不含自己"
    );
    assert!(chain.iter().all(|id| *id != scene));

    // 根级节点没有祖先
    assert!(book.store.node_ancestors(book.volumes[0]).unwrap().is_empty());
    // 不存在的节点要明确报错，而不是给一条空链
    assert!(book.store.node_ancestors(999_999).is_err());
}

/// 回归：**标题里带章名时，也要认得出编号**。
///
/// 真踩过（用户 2026-09-13 报）：第二卷里已有「第6章 灯 … 第10章 灯」这种最常见的写法，
/// 点「+」新建时却从「第6章」重新数起——旧实现要求标题**以「章」结尾**才认编号，
/// 认不出就退回"同层现有几章 + 1"；连点几次就成了 第6…第16 章排在一起（看着像章号倒着长）。
#[test]
fn new_chapters_continue_the_numbering_even_when_titles_carry_a_name() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;

    let mut last = volume;
    for serial in 6..=10 {
        let title = format!("第{serial}章 {}", if serial % 2 == 0 { "灯" } else { "门" });
        last = store.create_node(work.id, Some(volume), NodeKind::Chapter, &title).unwrap();
        store.write_body(last, "正文。").unwrap();
    }

    // 在第10章后面点「+」（标题留空＝按同层取号）：要继续数到第11章，而不是回到第6章
    let created = store.add_chapter_after(last, NodeKind::Chapter, "").unwrap();
    assert_eq!(store.node_title(created).unwrap(), "第11章");

    // 再点一次：继续往上数（而不是又算成第7章）
    let again = store.add_chapter_after(created, NodeKind::Chapter, "").unwrap();
    assert_eq!(store.node_title(again).unwrap(), "第12章");
}
