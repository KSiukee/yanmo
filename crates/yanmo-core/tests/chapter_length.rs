//! 章长判据的验收：**样本门槛 / 中位数与倍数 / 自适应 / 不偏长就安静**，
//! 以及"就这样写"之后这一章不再提。
//!
//! 判据的**唯一**出处是 `yanmo_core::question::rhythm`：叩问那条 `rhythm.chapter_very_long`
//! 与界面那条章长提醒读的都是它。所以其中一条测试特意走"同一本书、两条线"——
//! 叩问草稿里那个字数与界面那一屏的判定必须是同一个数（两套判据会在那里当场分家）。

use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::question::rhythm::{self, ChapterLengths, MIN_SAMPLES};
use yanmo_core::question::{generate, ChapterFacts, RhythmParams};
use yanmo_core::store::{dismiss_identity, Store};

/// 一章的纯事实（判据只认其中两个字段：写了没有、多少字）。
fn chapter(id: i64, chars: i64) -> ChapterFacts {
    ChapterFacts {
        node_id: id,
        title: format!("第{id}章"),
        has_body: chars > 0,
        char_count: chars,
        summary: String::new(),
    }
}

/// 被判成"偏长"的那几章（按原顺序）。
fn long_ids(lengths: &ChapterLengths) -> Vec<i64> {
    lengths
        .chapters
        .iter()
        .filter(|item| item.long)
        .map(|item| item.node_id)
        .collect()
}

#[test]
fn fewer_samples_than_the_gate_never_judges() {
    // 两章看着悬殊（1000 vs 9000），但那还构不成"这本书的分布"
    let chapters = vec![chapter(1, 1000), chapter(2, 9000)];
    let dist = rhythm::distribution(&chapters);
    assert!(!dist.judged, "样本不到 {MIN_SAMPLES} 章：不判");
    assert_eq!(
        dist.median, 0,
        "不判时中位数如实报 0（不拿两章的平均数糊上去）"
    );
    assert!(!rhythm::exceeds(9000, &dist, 2.0), "样本不足 ⇒ 一律不超");
    assert!(
        long_ids(&rhythm::judge(&chapters, 2.0)).is_empty(),
        "一章都不标偏长"
    );
}

#[test]
fn median_and_ratio_are_exact() {
    // 四章样本 ⇒ 上中位数落在 3000（不是 2000 与 3000 的平均 2500）
    let chapters = vec![
        chapter(1, 1000),
        chapter(2, 2000),
        chapter(3, 3000),
        chapter(4, 4000),
    ];
    let dist = rhythm::distribution(&chapters);
    assert!(dist.judged);
    assert_eq!(
        dist.median, 3000,
        "上中位数：偶数个取靠后的那个（落在真有的一章上）"
    );

    // 倍数 2.0 ⇒ 门槛 6000：踩线不算，越过一线才算
    let mut at_the_line = chapters.clone();
    at_the_line.push(chapter(5, 6000));
    assert!(
        long_ids(&rhythm::judge(&at_the_line, 2.0)).is_empty(),
        "正好中位数两倍：不算（严格大于）"
    );

    let mut over_the_line = chapters.clone();
    over_the_line.push(chapter(5, 6001));
    assert_eq!(
        long_ids(&rhythm::judge(&over_the_line, 2.0)),
        vec![5],
        "越过一线就算"
    );

    // 倍数就是那个旋钮：调到 3 倍，同一章不再算特别长
    assert!(
        long_ids(&rhythm::judge(&over_the_line, 3.0)).is_empty(),
        "3 倍之下 6001 不算特别长"
    );
}

#[test]
fn a_run_of_long_chapters_moves_the_threshold() {
    // 一开始只有一章长（别的是 1000）：中位 1000 ⇒ 门槛 2000 ⇒ 5000 偏长
    let before = vec![
        chapter(1, 1000),
        chapter(2, 1000),
        chapter(3, 1000),
        chapter(4, 5000),
    ];
    assert_eq!(rhythm::distribution(&before).median, 1000);
    assert_eq!(long_ids(&rhythm::judge(&before, 2.0)), vec![4]);

    // 他连着几章都这么写：分布跟着挪（中位 5000 ⇒ 门槛 10000），5000 不再"特别长"
    let after = vec![
        chapter(1, 1000),
        chapter(2, 1000),
        chapter(3, 1000),
        chapter(4, 5000),
        chapter(5, 5000),
        chapter(6, 5000),
        chapter(7, 5000),
    ];
    assert_eq!(
        rhythm::distribution(&after).median,
        5000,
        "连续几章都超长 ⇒ 阈值跟着他自己走"
    );
    assert!(
        long_ids(&rhythm::judge(&after, 2.0)).is_empty(),
        "分布变了就自然不说了（自适应）"
    );
}

#[test]
fn chapters_in_the_normal_band_stay_quiet() {
    let chapters = vec![
        chapter(1, 1800),
        chapter(2, 2000),
        chapter(3, 2200),
        chapter(4, 2400),
    ];
    assert_eq!(rhythm::distribution(&chapters).median, 2200);
    assert!(
        long_ids(&rhythm::judge(&chapters, 2.0)).is_empty(),
        "都在正常幅度里：一声不响"
    );
}

#[test]
fn empty_chapters_are_not_samples_and_never_long() {
    // 空章（还没落笔）不进分布，也不会被标成偏长
    let chapters = vec![
        chapter(1, 1000),
        chapter(2, 1000),
        chapter(3, 1000),
        chapter(4, 0),
    ];
    assert_eq!(
        rhythm::distribution(&chapters).median,
        1000,
        "没写正文的章不进样本"
    );
    assert!(long_ids(&rhythm::judge(&chapters, 2.0)).is_empty());
}

// ── 存储层那一屏：读数、与叩问同源、"就这样写" ────────────────────────────

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 造一本一卷的书，按给定的字数逐章落正文（0 = 空章），返回（作品 id, 章节 id）。
fn book_with(store: &mut Store, sizes: &[usize]) -> (i64, Vec<i64>) {
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let mut chapters = Vec::new();
    for size in sizes {
        let id = store
            .create_node(work.id, Some(volume), NodeKind::Chapter, "")
            .unwrap();
        if *size > 0 {
            store.write_body(id, &"字".repeat(*size)).unwrap();
        }
        chapters.push(id);
    }
    (work.id, chapters)
}

#[test]
fn the_store_board_and_the_question_rule_share_one_judge() {
    let (_dir, mut store) = fresh();
    let (work, chapters) = book_with(&mut store, &[1000, 1000, 1000, 5000]);
    let board = store.chapter_lengths(work).unwrap();

    assert_eq!(board.median, 1000, "中位数");
    assert!(board.judged, "四章样本够");
    assert_eq!(board.ratio, 2.0, "倍数与叩问那个旋钮同一个默认值");
    assert_eq!(board.min_samples, MIN_SAMPLES);
    assert_eq!(board.chapters[3].chars, 5000, "事实念得准：这一章多少字");
    let long: Vec<i64> = board
        .chapters
        .iter()
        .filter(|c| c.long)
        .map(|c| c.node_id)
        .collect();
    assert_eq!(long, vec![chapters[3]]);

    // **同源**：叩问那条规则报出来的字数与界面这一屏说的是同一个数
    let elements = store.writing_elements(work).unwrap();
    let drafts = generate(&elements, &RhythmParams::default());
    let rule = drafts
        .iter()
        .find(|draft| draft.template_key == "rhythm.chapter_very_long")
        .expect("这一章该被叩问问一条");
    assert_eq!(rule.slots["chars"], "5000", "两条线同一个判据 ⇒ 同一个数");
    assert_eq!(rule.anchors, vec![format!("rhythm:long:{}", chapters[3])]);
}

#[test]
fn keep_marks_this_chapter_only() {
    let (_dir, mut store) = fresh();
    let (work, chapters) = book_with(&mut store, &[1000, 1000, 1000, 5000]);
    let mark = dismiss_identity(chapters[3]);
    assert_eq!(
        mark,
        format!("chapter-long:{}", chapters[3]),
        "记号一眼看得出是「章长」这条规则"
    );

    store.dismiss_issue(work, &mark, "test").unwrap();

    let board = store.chapter_lengths(work).unwrap();
    let kept = board
        .chapters
        .iter()
        .find(|c| c.node_id == chapters[3])
        .unwrap();
    assert!(
        kept.long,
        "忽略的是「还提不提」，不是事实本身——这一章确实偏长"
    );
    assert!(kept.dismissed, "说过「就这样写」：界面以后不再提它");
    assert_eq!(
        kept.fingerprint, mark,
        "界面要写就用这一份原文照抄（不自己拼 identity）"
    );
    assert!(
        board
            .chapters
            .iter()
            .filter(|c| c.node_id != chapters[3])
            .all(|c| !c.dismissed),
        "别的章照旧"
    );
}
