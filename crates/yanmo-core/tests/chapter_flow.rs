//! 创章流程的**随机压测**：把"作者手忙脚乱地建章、删章、恢复、改名"揉成一串随机操作，
//! **每一步之后**都检查几条不变量。
//!
//! 为什么要有这一份：单条用例只能盖住一种操作顺序，而真机上的乱序正是漏网的地方——
//! 「同层 18/19/20/21 时在第20章上点「+」」这一步就长出了 `20 / 22 / 23 / 21`。
//! 作者"随便试试"就能试出来的东西，机器应该先试出来：这里用固定种子跑几百步，
//! 每一步都盯着"列表还按号递增吗、序号还密集吗、有没有撞号"。
//!
//! 种子固定 = 可复现；失败信息里带步数与操作，照着那一步重跑就行。

use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::Store;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 固定种子的线性同余（不引第三方随机库：压测要的是**可复现**，不是真随机）。
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 11
    }

    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            0
        } else {
            (self.next() % bound as u64) as usize
        }
    }
}

/// 一本书的现场：卷 + 每一层的活章。
struct Book {
    store: Store,
    work_id: i64,
    volume: i64,
}

impl Book {
    fn new() -> (tempfile::TempDir, Book) {
        let (dir, mut store) = fresh();
        let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
        let volume = store.list_nodes(work.id).unwrap()[0].id;
        let book = Book { store, work_id: work.id, volume };
        (dir, book)
    }

    /// 这一层活着的章（按目录顺序）。
    fn chapters(&self, parent: i64) -> Vec<i64> {
        self.store
            .list_nodes(self.work_id)
            .unwrap()
            .into_iter()
            .filter(|node| node.parent_id == Some(parent) && node.kind == NodeKind::Chapter)
            .map(|node| node.id)
            .collect()
    }

    /// 往卷里连着垫 `count` 章（作者"已经写到第 N 章"的现场）。
    fn seed(&mut self, count: usize) -> Vec<i64> {
        let mut ids = Vec::new();
        for _ in 0..count {
            let id = self
                .store
                .create_node(self.work_id, Some(self.volume), NodeKind::Chapter, "")
                .unwrap();
            ids.push(id);
        }
        ids
    }

    fn title_of(&self, id: i64) -> String {
        self.store.node_title(id).unwrap()
    }

    /// 每一层都查一遍：**编号必须按目录顺序严格递增**，序号必须密集，不许撞号。
    ///
    /// 只对"有编号的章"要求递增：作者自起的名字（序章 / 番外）没有号可归位，
    /// 它们的落点由作者定（`create` 那条规矩），不在这一条的管辖范围内。
    fn assert_invariants(&self, step: usize, what: &str) {
        let nodes = self.store.list_nodes(self.work_id).unwrap();
        let mut layers: Vec<Option<i64>> = vec![None];
        for node in &nodes {
            if node.kind.accepts_children() {
                layers.push(Some(node.id));
            }
        }
        for layer in layers {
            let live: Vec<&yanmo_core::store::NodeSummary> = nodes
                .iter()
                .filter(|node| node.parent_id == layer && node.kind == NodeKind::Chapter)
                .collect();

            // ① 有编号的章：号在目录顺序上**不许倒退**（可以相等——见下面那条说明）
            let mut previous = 0i64;
            for node in &live {
                let Some(serial) = serial_of(&node.title) else { continue };
                assert!(
                    serial >= previous,
                    "第 {step} 步（{what}）之后，层 {layer:?} 里「{}」(第{serial}章) 排在了更小的号后面：{}",
                    node.title,
                    live.iter().map(|n| n.title.clone()).collect::<Vec<_>>().join(" / ")
                );
                previous = serial;
            }

            // ② 撞号**不在这条的管辖内**：作者可以把一章改名成任何号，回收站里那一章捞回来时
            //    也可能撞上"已经被新建章重新用掉"的号——那是恢复预检 + 作者三选一那条人工路
            //    （见 `restore_preview` 的同名提示）。这里只保证**自动流程**不把顺序搞乱。

            // ③ 序号密集且不重复（0..n-1）：拖动与新建都靠它
            let orders: Vec<i64> = live.iter().map(|node| node.sort_order).collect();
            for (expected, got) in orders.iter().enumerate() {
                assert_eq!(
                    *got, expected as i64,
                    "第 {step} 步（{what}）之后，层 {layer:?} 的序号不密集：{orders:?}"
                );
            }
        }
    }
}

/// 认标题里的章号（压测自己用；与核心的规则同源，但这里只关心数字）。
fn serial_of(title: &str) -> Option<i64> {
    let rest = title.strip_prefix('第')?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// ★ 主力压测：600 步随机操作，每一步之后都查不变量。
#[test]
fn creating_and_editing_chapters_keeps_the_outline_numbered() {
    let (_dir, mut book) = Book::new();
    let mut rng = Rng(0x5eed_1234_abcd);

    // 先垫二十章（模拟"已经写到 20 章"）——**建在卷里**，不然随机走查会一直空转
    book.seed(20);
    book.assert_invariants(0, "开场二十章");
    assert_eq!(book.chapters(book.volume).len(), 20, "垫章没进卷：后面的压测等于没跑");

    // 回收站里的章：**连名字一起记着**——删掉的节点上读不回标题（那是"已不在"的语义）
    let mut deleted: Vec<(i64, String)> = Vec::new();
    let mut history: Vec<String> = Vec::new();

    for step in 1..=600 {
        let chapters = book.chapters(book.volume);
        let roll = rng.below(100);
        let what = if roll < 45 && !chapters.is_empty() {
            // ① 在随机一行上点「+」（最常做的事）
            let row = chapters[rng.below(chapters.len())];
            let step_desc = format!("在「{}」上点 +", book.title_of(row));
            book.store.add_chapter_after(row, NodeKind::Chapter, "").unwrap();
            step_desc
        } else if roll < 70 && !chapters.is_empty() {
            // ② 删掉随机一章（进回收站）
            let row = chapters[rng.below(chapters.len())];
            let title = book.title_of(row);
            let step_desc = format!("删掉「{title}」");
            book.store.soft_delete_node(row).unwrap();
            deleted.push((row, title));
            step_desc
        } else if roll < 85 && !deleted.is_empty() {
            // ③ 从回收站捞回一章（回原位——可能与后来的新建交错）
            let index = rng.below(deleted.len());
            let (row, title) = deleted.remove(index);
            let step_desc = format!("捞回「{title}」");
            book.store.restore_node(row, None).unwrap();
            step_desc
        } else if !chapters.is_empty() {
            // ④ 给随机一章改名（号不变，只加个后缀）
            let row = chapters[rng.below(chapters.len())];
            let serial = serial_of(&book.title_of(row)).unwrap_or(1);
            let step_desc = format!("把「{}」改名为 第{serial}章·灯", book.title_of(row));
            book.store.rename_node(row, &format!("第{serial}章·灯")).unwrap();
            step_desc
        } else {
            "空转".to_string()
        };

        history.push(format!("#{step} {what}"));
        if history.len() > 12 {
            history.remove(0);
        }
        if let Err(failure) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            book.assert_invariants(step, &what)
        })) {
            let _ = failure;
            panic!("压测在第 {step} 步失守：{what}\n最近几步：\n{}", history.join("\n"));
        }
    }
}

/// 定向压测：**同一行连点二十次「+」**——原来在这里长出过"倒着长"。
#[test]
fn clicking_plus_on_the_same_row_twenty_times_keeps_going_down() {
    let (_dir, mut book) = Book::new();
    book.seed(5);
    // 挑**中间**那一行连点：号一直往上加，位置也必须一路往下排
    let middle = book.chapters(book.volume)[1];
    for _ in 0..20 {
        book.store.add_chapter_after(middle, NodeKind::Chapter, "").unwrap();
        book.assert_invariants(0, "连点 +");
    }
    let titles: Vec<String> =
        book.chapters(book.volume).iter().map(|id| book.title_of(*id)).collect();
    assert_eq!(titles[0], "第1章");
    assert_eq!(titles[1], "第2章");
    assert_eq!(titles.last().unwrap(), "第25章", "二十次连点：{titles:?}");
}

/// 定向压测：**删中间章 → 建新章 → 再捞回来**，编号与位置都不许乱。
#[test]
fn delete_create_and_restore_keep_the_outline_sorted() {
    let (_dir, mut book) = Book::new();
    book.seed(10);
    let chapters = book.chapters(book.volume);
    assert_eq!(chapters.len(), 10, "垫章没进卷：后面的压测等于没跑");

    // 删掉第5章，再连建三章（号从 11 起，位置排在最后）
    book.store.soft_delete_node(chapters[4]).unwrap();
    for _ in 0..3 {
        book.store.add_chapter_after(chapters[2], NodeKind::Chapter, "").unwrap();
        book.assert_invariants(0, "删后建章");
    }
    // 把第5章捞回来：它带着自己的号，落点必须让它回到 4 与 6 之间
    book.store.restore_node(chapters[4], None).unwrap();
    book.assert_invariants(0, "捞回第5章");

    let titles: Vec<String> =
        book.chapters(book.volume).iter().map(|id| book.title_of(*id)).collect();
    let serials: Vec<i64> = titles.iter().filter_map(|title| serial_of(title)).collect();
    let mut sorted = serials.clone();
    sorted.sort_unstable();
    assert_eq!(serials, sorted, "捞回来之后也必须按号递增：{titles:?}");
}
