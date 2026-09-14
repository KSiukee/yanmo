//! 每日码字账本验收：**今日进度 / 码字日历**背后的那条流水。
//!
//! 六条判据：
//! 1. 只有**编辑器落盘**（`write_body_counted`）记账，净增减与作者眼前的字数变化一致；
//! 2. 快照回滚 / 普通写入这类"不是今天写的字"**不动账本**；
//! 3. 哪一天由作者时区定（同一笔在 +8 区与 UTC 可能落在不同的日子）；
//! 4. 每本书各记各的，也能合计着看；
//! 5. 区间聚合只回有记录的日子（空日子由界面补），边界是闭区间；
//! 6. 连续天数从今天往回数，今天还没写就从昨天数，断档就停。

use yanmo_core::model::WorkKind;
use yanmo_core::store::{Appearance, Store};
use yanmo_core::time::{local_date, now_millis};

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 新建一本书并拿到它的第一章。
fn book(store: &mut Store, title: &str) -> (i64, i64) {
    let work = store.create_work(WorkKind::Novel, title).unwrap();
    let chapter = store.list_nodes(work.id).unwrap()[0].id;
    (work.id, chapter)
}

/// `n` 天前的本地日期（按 UTC 算——测试里只用来构造"昨天 / 前天"）。
fn days_ago(n: i64) -> String {
    local_date(now_millis() - n * 86_400_000, 0)
}

/// 直接往账本里塞一天（构造历史用；记账本身走 `write_body_counted`）。
fn seed_day(store: &Store, work_id: i64, day: &str, chars: i64) {
    store
        .conn()
        .execute(
            "INSERT INTO writing_days(day, work_id, chars, chars_no_punct, words, updated_at)
             VALUES(?1, ?2, ?3, ?3, ?3, 0)",
            rusqlite::params![day, work_id, chars],
        )
        .unwrap();
}

#[test]
fn editor_saves_are_counted_as_net_change() {
    let (_dir, mut store) = fresh();
    let (work_id, chapter) = book(&mut store, "长夜");
    // 库里先有一版（不走记账，相当于导入的旧稿）——它只是基线，不该出现在今日进度里
    store.write_body(chapter, "雨下了整夜。").unwrap();
    assert_eq!(store.writing_today(Some(work_id), 0).unwrap().chars, 0);

    // 作者又写了 4 个字：只记这 4 个
    store.write_body_counted(chapter, "雨下了整夜，屋檐在滴水。", 0).unwrap();
    assert_eq!(store.writing_today(Some(work_id), 0).unwrap().chars, 6, "12 字减基线 6 字");

    // 同一章里删掉一段：扣回来（净增减，与状态栏的字数变化一致）
    store.write_body_counted(chapter, "雨下了整夜。", 0).unwrap();
    assert_eq!(store.writing_today(Some(work_id), 0).unwrap().chars, 0);

    // 再写一笔，仍记在同一行上（一天一行）
    store.write_body_counted(chapter, "雨下了整夜。天亮了。", 0).unwrap();
    let today = store.writing_today(Some(work_id), 0).unwrap();
    assert_eq!(today.chars, 4, "10 字减基线 6 字");
    let rows: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM writing_days", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 1, "同一天同一本书只该有一行");
}

#[test]
fn untracked_writes_and_zero_deltas_leave_the_ledger_alone() {
    let (_dir, mut store) = fresh();
    let (work_id, chapter) = book(&mut store, "长夜");

    // 普通写入（快照回滚 / 恢复备份走这条）不记账
    store.write_body(chapter, "第一版。").unwrap();
    assert_eq!(store.writing_today(Some(work_id), 0).unwrap().chars, 0);

    store.write_body_counted(chapter, "第一版加长。", 0).unwrap();
    assert_eq!(store.writing_today(Some(work_id), 0).unwrap().chars, 2);

    // 换一个同字数的写法：正文变了但增减是 0，账本不该多出一行
    store.write_body_counted(chapter, "第一版加短。", 0).unwrap();
    assert_eq!(store.writing_today(Some(work_id), 0).unwrap().chars, 2, "同字数换字：增减为 0，账本不动");

    // 内容没变时核心直接返回，账本也不动
    store.write_body_counted(chapter, "第一版加短。", 0).unwrap();
    assert_eq!(store.writing_today(Some(work_id), 0).unwrap().chars, 2, "同字数换字：增减为 0，账本不动");

    // 紧急抢救算数（那些字是作者刚敲的，上一次落盘没成功才走到这里）
    store.emergency_snapshot(chapter, "抢救回来的一段话。", "desync", 0).unwrap();
    assert!(store.writing_today(Some(work_id), 0).unwrap().chars > 2);
}

#[test]
fn the_day_bucket_follows_the_author_timezone() {
    let (_dir, mut store) = fresh();
    let (_, chapter) = book(&mut store, "长夜");
    let (_, other) = book(&mut store, "短歌");

    store.write_body_counted(chapter, "东八区写的。", 480).unwrap();
    store.write_body_counted(other, "UTC 写的。", 0).unwrap();

    let mut stmt = store
        .conn()
        .prepare("SELECT day FROM writing_days ORDER BY day")
        .unwrap();
    let days: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    for day in &days {
        assert!(
            *day == local_date(now_millis(), 480) || *day == local_date(now_millis(), 0),
            "账本里的日子必须是调用方时区算出来的：{day}"
        );
    }
    // 窗口要用**账本里真实存在的日子**来定：+8 区那一笔可能落在 UTC 的"明天"，
    // 而 `days_ago(0)` 是按 UTC 数的——上界用它会在本地午夜前后的 8 小时里漏掉那一笔。
    // 这条断言原来就是这么挂的（真实时钟 + 真机时区撞出来的），别再写回"按今天数窗口"。
    let (Some(first), Some(last)) = (days.first(), days.last()) else {
        panic!("两笔都该记进账本：{days:?}");
    };
    assert_eq!(
        store.writing_between(None, first, last).unwrap().len(),
        if local_date(now_millis(), 480) == local_date(now_millis(), 0) { 1 } else { 2 },
        "两笔按各自时区分桶：同一天就并成一行，不同天就是两行"
    );
}

#[test]
fn books_keep_their_own_ledger_and_also_sum_up() {
    let (_dir, mut store) = fresh();
    let (first, a) = book(&mut store, "长夜");
    let (second, b) = book(&mut store, "短歌");
    store.write_body_counted(a, "三字。", 0).unwrap();
    store.write_body_counted(b, "五个字的正文。", 0).unwrap();

    assert_eq!(store.writing_today(Some(first), 0).unwrap().chars, 3);
    assert_eq!(store.writing_today(Some(second), 0).unwrap().chars, 7);
    assert_eq!(
        store.writing_today(None, 0).unwrap().chars,
        10,
        "不指定书就是全部作品合计"
    );
}

#[test]
fn calendar_range_is_closed_and_skips_empty_days() {
    let (_dir, mut store) = fresh();
    let (work_id, chapter) = book(&mut store, "长夜");
    store.write_body_counted(chapter, "今天写的。", 0).unwrap();
    seed_day(&store, work_id, &days_ago(3), 100);
    seed_day(&store, work_id, &days_ago(1), 50);
    seed_day(&store, work_id, &days_ago(9), 999);

    let days = store.writing_between(Some(work_id), &days_ago(3), &days_ago(1)).unwrap();
    assert_eq!(days.len(), 2, "区间外的第 9 天不进来，第 0 天也在区间外");
    assert_eq!(days[0].day, days_ago(3));
    assert_eq!(days[0].chars, 100);
    assert_eq!(days[1].chars, 50);
    // 只有一端等于边界也是闭区间
    let edge = store.writing_between(Some(work_id), &days_ago(3), &days_ago(3)).unwrap();
    assert_eq!(edge.len(), 1);
}

#[test]
fn streak_counts_back_from_today_and_stops_at_a_gap() {
    let (_dir, mut store) = fresh();
    let (work_id, _chapter) = book(&mut store, "长夜");

    // 昨天、前天写过，今天还没写：连着 2 天（不该一早起来就断签）
    seed_day(&store, work_id, &days_ago(1), 10);
    seed_day(&store, work_id, &days_ago(2), 10);
    assert_eq!(store.writing_streak(Some(work_id), 0).unwrap(), 2);

    // 再往前断了一天：只数到前天
    seed_day(&store, work_id, &days_ago(4), 10);
    assert_eq!(store.writing_streak(Some(work_id), 0).unwrap(), 2);

    // 今天写了：连续 3 天
    seed_day(&store, work_id, &days_ago(0), 10);
    assert_eq!(store.writing_streak(Some(work_id), 0).unwrap(), 3);

    // 净删的一天不算"写了"
    seed_day(&store, work_id, &days_ago(3), -5);
    assert_eq!(store.writing_streak(Some(work_id), 0).unwrap(), 3, "断档照旧停在这里");
}

#[test]
fn daily_goal_is_global_with_a_per_book_override() {
    let (_dir, mut store) = fresh();
    let (first, _) = book(&mut store, "长夜");
    let (second, _) = book(&mut store, "短歌");

    assert_eq!(store.appearance(Some(first)).unwrap().daily_goal, None, "没设过就没有目标");

    store
        .set_appearance(None, &Appearance { daily_goal: Some(2000), ..Default::default() })
        .unwrap();
    assert_eq!(store.appearance(None).unwrap().daily_goal, Some(2000));
    assert_eq!(store.appearance(Some(first)).unwrap().daily_goal, Some(2000), "继承全局");
    assert_eq!(store.appearance(Some(second)).unwrap().daily_goal, Some(2000));

    store
        .set_appearance(Some(first), &Appearance { daily_goal: Some(500), ..Default::default() })
        .unwrap();
    assert_eq!(store.appearance(Some(first)).unwrap().daily_goal, Some(500), "这本书单独设");
    assert_eq!(store.appearance(Some(second)).unwrap().daily_goal, Some(2000), "另一本不受影响");

    // 0 / 负数 = 清掉目标；离谱的大数夹在上限（手滑多打几个零）
    store
        .set_appearance(Some(first), &Appearance { daily_goal: Some(0), ..Default::default() })
        .unwrap();
    assert_eq!(store.appearance(Some(first)).unwrap().daily_goal, Some(2000), "清掉覆盖 = 回到全局");
    store
        .set_appearance(None, &Appearance { daily_goal: Some(99_999_999), ..Default::default() })
        .unwrap();
    assert_eq!(store.appearance(None).unwrap().daily_goal, Some(1_000_000));
    store
        .set_appearance(None, &Appearance { daily_goal: Some(-1), ..Default::default() })
        .unwrap();
    assert_eq!(store.appearance(None).unwrap().daily_goal, None, "负数当没设目标");
}

#[test]
fn counts_never_leak_across_books_via_a_stale_baseline() {
    let (_dir, mut store) = fresh();
    let (first, a) = book(&mut store, "长夜");
    let (second, b) = book(&mut store, "短歌");
    // 两本书里同名同字数的正文：基线必须各取各的
    store.write_body(a, "同样的开头。").unwrap();
    store.write_body(b, "同样的开头。").unwrap();
    store.write_body_counted(a, "同样的开头。补一句。", 0).unwrap();
    assert_eq!(store.writing_today(Some(first), 0).unwrap().chars, 4);
    assert_eq!(store.writing_today(Some(second), 0).unwrap().chars, 0, "另一本书不该被动到");
}
