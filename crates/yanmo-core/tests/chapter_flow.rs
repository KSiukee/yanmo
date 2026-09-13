//! 创章流程的**随机压测**：把"作者手忙脚乱地建章、删章、恢复、改名"揉成一串随机操作，
//! **每一步之后**都检查"渲染出来的章号是否仍然按位置 1、2、3… 递增"。
//!
//! # 这一份盯的是什么
//!
//! 号是**位置的函数**（标题里写 `第{$N}章`，显示时按同层位置渲染，见 `crate::numbering`）。
//! 所以真正要保住的性质只有一条：
//!
//! > 同层里"要编号的章"，渲染出来的号必须正好是 1、2、3…、n，顺序与目录一致，不跳号不倒退。
//!
//! 插入、删除、从回收站捞回、把某章改名成 `序章`（去掉宏，从此不占号）——**任何顺序组合**
//! 都不许破坏它。真机上出过的毛病（同层 18/19/20/21 时点「+」得到 `20 / 22 / 23 / 21`）
//! 就是这条被破坏的样子；单条用例只盖得住一种顺序，随机走几百步才盖得全。
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

    /// 往卷里连着垫 `count` 章（标题留空＝用核心的模板 `第{$N}章`）。
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

    /// 这一层的行（按目录顺序）：`(id, 原文, 渲染后, 层内序号)`——**一次查全**。
    fn rows(&self) -> Vec<(i64, String, String, i64)> {
        self.store
            .list_nodes(self.work_id)
            .unwrap()
            .into_iter()
            .filter(|node| node.parent_id == Some(self.volume) && node.kind == NodeKind::Chapter)
            .map(|node| (node.id, node.title, node.title_rendered, node.sort_order))
            .collect()
    }

    fn ids(&self) -> Vec<i64> {
        self.rows().iter().map(|row| row.0).collect()
    }

    fn title_of(&self, id: i64) -> String {
        self.rows().iter().find(|row| row.0 == id).map(|row| row.1.clone()).unwrap_or_default()
    }

    fn rendered_of(&self, id: i64) -> String {
        self.rows().iter().find(|row| row.0 == id).map(|row| row.2.clone()).unwrap_or_default()
    }

    fn rendered_all(&self) -> Vec<String> {
        self.rows().iter().map(|row| row.2.clone()).collect()
    }

    /// **唯一必须保住的不变量**：要编号的章，渲染出来正好是 1、2、3…n。
    ///
    /// 顺带查"同层次序号必须密集"（拖动与新建都靠它）与"显示出来的标题里不许还留着宏"。
    fn assert_invariants(&self, step: usize, what: &str) {
        let rows = self.rows();
        let serials: Vec<i64> =
            rows.iter().filter_map(|row| serial_of(&row.2)).collect();
        let want: Vec<i64> = (1..=serials.len() as i64).collect();
        assert_eq!(
            serials,
            want,
            "第 {step} 步（{what}）之后，渲染色号不是 1..n（目录：{:?}）",
            rows.iter().map(|row| row.2.clone()).collect::<Vec<_>>()
        );

        for row in &rows {
            assert!(
                !row.2.contains("{$"),
                "第 {step} 步（{what}）之后，显示出来的标题里还留着宏：{}",
                row.2
            );
        }

        for (expected, row) in rows.iter().enumerate() {
            assert_eq!(
                row.3, expected as i64,
                "第 {step} 步（{what}）之后序号不密集：{:?}",
                rows.iter().map(|row| row.3).collect::<Vec<_>>()
            );
        }
    }
}

/// 从渲染后的标题里读号（压测自己用的小解析器：只认「第<数字>…」）。
fn serial_of(title: &str) -> Option<i64> {
    let rest = title.strip_prefix('第')?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// ★ 主力压测：600 步随机操作，每一步之后都查"号还是 1..n"。
#[test]
fn creating_and_editing_chapters_keeps_the_numbers_in_step_with_positions() {
    let (_dir, mut book) = Book::new();
    let mut rng = Rng(0x5eed_1234_abcd);

    book.seed(20); // 模拟"已经写到 20 章"
    book.assert_invariants(0, "开场二十章");
    assert_eq!(book.rendered_all().len(), 20, "垫章没进卷：后面的压测等于没跑");

    // 回收站里的章：**连名字一起记着**（删掉的节点上读不回标题）
    let mut deleted: Vec<(i64, String)> = Vec::new();
    let mut history: Vec<String> = Vec::new();

    for step in 1..=600 {
        let chapters = book.ids();
        let roll = rng.below(100);
        let what = if roll < 45 && !chapters.is_empty() {
            let row = chapters[rng.below(chapters.len())];
            let step_desc = format!("在「{}」上点 +", book.rendered_of(row));
            book.store.add_chapter_after(row, NodeKind::Chapter, "").unwrap();
            step_desc
        } else if roll < 65 && !chapters.is_empty() {
            let row = chapters[rng.below(chapters.len())];
            let title = book.title_of(row);
            let step_desc = format!("删掉「{}」", book.rendered_of(row));
            book.store.soft_delete_node(row).unwrap();
            deleted.push((row, title));
            step_desc
        } else if roll < 85 && !deleted.is_empty() {
            let index = rng.below(deleted.len());
            let (row, title) = deleted.remove(index);
            let step_desc = format!("捞回「{title}」");
            book.store.restore_node(row, None).unwrap();
            step_desc
        } else if !chapters.is_empty() {
            // 作者给某一章起自己的名字（宏被替换掉，这一章从此不占号——序章 / 番外那种）
            let row = chapters[rng.below(chapters.len())];
            let step_desc = format!("把「{}」改名成 番外·夜谈", book.rendered_of(row));
            book.store.rename_node(row, "番外·夜谈").unwrap();
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

/// 定向压测：**同一行连点二十次「+」**——号必须一路往下（1、2、3…），位置也一路往下。
#[test]
fn clicking_plus_on_the_same_row_twenty_times_keeps_going_down() {
    let (_dir, mut book) = Book::new();
    book.seed(5);
    let middle = book.ids()[1];
    for _ in 0..20 {
        book.store.add_chapter_after(middle, NodeKind::Chapter, "").unwrap();
        book.assert_invariants(0, "连点 +");
    }
    let shown = book.rendered_all();
    assert_eq!(shown[0], "第1章");
    assert_eq!(shown[1], "第2章");
    assert_eq!(shown.last().unwrap(), "第25章", "二十次连点：{shown:?}");
}

/// 定向压测：**删中间章 → 建新章 → 再捞回来**，号不许跳、不许倒退。
#[test]
fn delete_create_and_restore_keep_the_numbers_dense() {
    let (_dir, mut book) = Book::new();
    book.seed(10);
    let chapters = book.ids();

    book.store.soft_delete_node(chapters[4]).unwrap();
    book.assert_invariants(0, "删掉第5章");
    for _ in 0..3 {
        book.store.add_chapter_after(chapters[2], NodeKind::Chapter, "").unwrap();
        book.assert_invariants(0, "删后建章");
    }
    book.store.restore_node(chapters[4], None).unwrap();
    book.assert_invariants(0, "捞回第5章");

    // 捞回来之后：它带着模板回到原位，号自己就对上了（不需要任何"按号归位"）
    let shown = book.rendered_all();
    assert_eq!(shown[4], "第5章", "捞回来的那一章就该显示第5章：{shown:?}");
}

/// 定向压测：把一章改名成"序章"（去掉宏）→ 它不占号，后面的号**整体前移**且仍旧连续。
#[test]
fn renaming_a_chapter_to_a_plain_name_frees_its_number() {
    let (_dir, mut book) = Book::new();
    book.seed(4);
    let chapters = book.ids();
    book.store.rename_node(chapters[1], "序章").unwrap();
    book.assert_invariants(0, "把第2章改名成序章");

    let shown = book.rendered_all();
    assert_eq!(shown, ["第1章", "序章", "第2章", "第3章"], "序章不占号，后面的号往前顶");
}
