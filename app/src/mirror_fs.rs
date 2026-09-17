//! 镜像**真碰磁盘**的那一半：探一份文件是谁写的、按计划写 / 改名 / 收残留。
//!
//! # 为什么单独一份
//!
//! 它不碰数据库、不碰线程、不做任何判断——只把 [`crate::mirror`] 算好的计划落到盘上。
//! 分出来有两个好处：一是"改名 / 收残留 / 空目录"这些**只有真磁盘才看得出的行为**
//! 可以在本文件里用临时目录铺开测（见文件末尾），二是线程那一层从此只管节拍与报告。
//!
//! # 两条不让步的规矩
//!
//! - **只读一份、只写一份**：探测一份文件要读它的字节算指纹；写永远是**原子写**
//!   （同目录临时文件 + `rename`），所以从盘上看到的要么是旧的那份整的、要么是新的整的。
//! - **收残留绝不越出镜像根**：`prune_empty_dirs` 从被删文件的父目录往上收，
//!   碰到非空或到根就停；镜像根之外一个目录都不碰。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use yanmo_core::store::{DiskState, MirrorAction, MirrorEntry, MirrorFile, MirrorPlan};
use yanmo_core::text::content_hash;

use crate::error::ApiError;

/// 某一份文件现在是"不在 / 是我们写的 / 是别人写的"。
pub(crate) fn probe(root: &Path, path: &str, account: &HashMap<&str, &MirrorEntry>) -> DiskState {
    let bytes = match std::fs::read(root.join(path)) {
        Ok(bytes) => bytes,
        Err(_) => return DiskState::Missing,
    };
    match account.get(path) {
        Some(entry) if content_hash(&String::from_utf8_lossy(&bytes)) == entry.file_hash => {
            DiskState::Ours
        }
        // 账上没有它、或账上记的不是这一份：**那就是别人的东西**（绝不覆盖）
        _ => DiskState::Foreign,
    }
}

/// 账上那些文件在磁盘上都还在、大小也对得上吗（**只 stat，不读内容**）。
///
/// 这是空闲巡检用的廉价探针：文件被删、被换成别的长度的东西，都能当场发现；
/// 长度一样的外部改动要等全量核对（启动 / 手动同步）才认得出——那是明写的取舍。
pub(crate) fn disk_looks_untouched(root: &Path, state: &[MirrorEntry]) -> bool {
    state.iter().filter(|entry| !entry.conflict).all(|entry| {
        std::fs::metadata(root.join(&entry.relative_path))
            .map(|meta| meta.is_file() && meta.len() == entry.size_bytes.max(0) as u64)
            .unwrap_or(false)
    })
}

/// 按计划落盘。顺序由核心定（先腾位置、再写、最后收），这里只管执行。
pub(crate) fn execute(
    root: &Path,
    desired: &[MirrorFile],
    plan: &MirrorPlan,
) -> Result<(), ApiError> {
    let files: HashMap<i64, &MirrorFile> =
        desired.iter().map(|file| (file.node_id, file)).collect();
    let mut emptied: Vec<PathBuf> = Vec::new();

    for action in &plan.actions {
        match action {
            MirrorAction::Rename { from, to, .. } => {
                let target = root.join(to);
                make_parent(&target)?;
                std::fs::rename(root.join(from), &target).map_err(|error| {
                    ApiError::with(
                        "shell.mirror_rename_failed",
                        [("path", target.display().to_string())],
                    )
                    .caused_by(error)
                })?;
            }
            MirrorAction::Write { node_id, relative_path } => {
                let Some(file) = files.get(node_id) else {
                    continue;
                };
                let target = root.join(relative_path);
                make_parent(&target)?;
                yanmo_core::atomic::write_atomic(&target, &file.content).map_err(|error| {
                    ApiError::with(
                        "shell.mirror_write_failed",
                        [("path", target.display().to_string())],
                    )
                    .caused_by(error)
                })?;
            }
            MirrorAction::Remove { relative_path, .. } => {
                let target = root.join(relative_path);
                match std::fs::remove_file(&target) {
                    Ok(()) => {
                        if let Some(parent) = target.parent() {
                            emptied.push(parent.to_path_buf());
                        }
                    }
                    // 已经不在就算了：目标状态就是"它不在"
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => {
                        return Err(ApiError::with(
                            "shell.mirror_remove_failed",
                            [("path", target.display().to_string())],
                        )
                        .caused_by(error));
                    }
                }
            }
        }
    }

    // 收掉因此空下来的目录（卷空了、整本书没了），但**绝不越过镜像根**
    for dir in emptied {
        prune_empty_dirs(&dir, root);
    }
    Ok(())
}

fn make_parent(path: &Path) -> Result<(), ApiError> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    std::fs::create_dir_all(parent).map_err(|error| {
        ApiError::with("shell.mirror_dir_create_failed", [("path", parent.display().to_string())])
            .caused_by(error)
    })
}

/// 自下而上收空目录；碰到非空（`remove_dir` 会失败）或到了根就停。
fn prune_empty_dirs(from: &Path, root: &Path) {
    let mut current = Some(from.to_path_buf());
    while let Some(dir) = current {
        if dir == root || !dir.starts_with(root) {
            return;
        }
        if std::fs::remove_dir(&dir).is_err() {
            return; // 还有东西，留着
        }
        current = dir.parent().map(Path::to_path_buf);
    }
}

#[cfg(test)]
mod tests {
    //! 落盘侧的验收：**真的写进一个临时目录再回来看**。
    //!
    //! 核心那边穷举的是"该做什么"（纯逻辑）；这里盯的是"真做出来是什么样"——
    //! 文件到底有没有、改名是不是真把旧文件挪走了、空目录有没有收掉、作者改过的那份
    //! 是不是**一个字节都没动**。这几条只有真碰磁盘才测得出。

    use super::*;
    use crate::mirror::MIRROR_DIR;
    use yanmo_core::model::{NodeKind, WorkKind};
    use yanmo_core::store::{plan_mirror, Store};

    fn setup() -> (tempfile::TempDir, Store, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("yanmo.db")).unwrap();
        let root = dir.path().join(MIRROR_DIR);
        (dir, store, root)
    }

    /// 一本两章的书（同一个根卷下）。
    fn book(store: &mut Store) -> (i64, Vec<i64>) {
        let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
        let volume = store.list_nodes(work.id).unwrap()[0].id;
        store.rename_node(volume, "第一卷").unwrap();
        let mut chapters = Vec::new();
        for (title, body) in [("第一章", "第一章的正文。"), ("第二章", "第二章的正文。")] {
            let id = store.create_node(work.id, Some(volume), NodeKind::Chapter, title).unwrap();
            store.write_body(id, body).unwrap();
            chapters.push(id);
        }
        (work.id, chapters)
    }

    /// 与壳里那条路一模一样：渲染 → 探磁盘 → 算计划 → 落盘 → 记账。
    fn reconcile(store: &mut Store, root: &Path, work: i64, verify: bool) -> MirrorPlan {
        let desired = store.render_mirror(work).unwrap();
        let state = store.mirror_state(work).unwrap();
        let account: HashMap<&str, &MirrorEntry> =
            state.iter().map(|entry| (entry.relative_path.as_str(), entry)).collect();
        let plan = plan_mirror(&desired, &state, |path| probe(root, path, &account), verify);
        execute(root, &desired, &plan).unwrap();
        store.mirror_record(work, &plan.records).unwrap();
        plan
    }

    fn read(root: &Path, path: &str) -> String {
        std::fs::read_to_string(root.join(path)).unwrap()
    }

    #[test]
    fn files_land_on_disk_and_a_second_pass_touches_nothing() {
        let (_dir, mut store, root) = setup();
        let (work, _) = book(&mut store);
        let first = reconcile(&mut store, &root, work, true);
        assert_eq!(first.actions.len(), 2, "两章两份：{:?}", first.actions);

        let path = format!("长夜-{work}/001-第一卷/001-第一章.md");
        assert_eq!(read(&root, &path), "# 第一章\n\n第一章的正文。\n");

        let second = reconcile(&mut store, &root, work, true);
        assert!(second.actions.is_empty(), "谁都没动过，第二次一个动作都不该有：{:?}", second.actions);
    }

    #[test]
    fn an_edited_file_is_left_byte_for_byte_alone() {
        let (_dir, mut store, root) = setup();
        let (work, _) = book(&mut store);
        reconcile(&mut store, &root, work, true);

        let path = format!("长夜-{work}/001-第一卷/001-第一章.md");
        std::fs::write(root.join(&path), "作者用记事本写的。\n").unwrap();
        let plan = reconcile(&mut store, &root, work, true);

        assert_eq!(read(&root, &path), "作者用记事本写的。\n", "外面的改动一个字节都不许动");
        assert_eq!(plan.conflicts.len(), 1);
        assert!(store.mirror_state(work).unwrap().iter().any(|entry| entry.conflict));
    }

    #[test]
    fn renaming_a_chapter_moves_the_file_and_leaves_no_orphan() {
        let (_dir, mut store, root) = setup();
        let (work, chapters) = book(&mut store);
        reconcile(&mut store, &root, work, true);

        let old = format!("长夜-{work}/001-第一卷/001-第一章.md");
        let new = format!("长夜-{work}/001-第一卷/001-灯.md");
        store.rename_node(chapters[0], "灯").unwrap();
        let plan = reconcile(&mut store, &root, work, true);

        assert!(
            plan.actions.iter().any(|action| matches!(action, MirrorAction::Rename { .. })),
            "改名就该是一次 rename：{:?}",
            plan.actions
        );
        assert!(!root.join(&old).exists(), "旧文件必须不在了（不留孤儿）");
        assert_eq!(read(&root, &new), "# 灯\n\n第一章的正文。\n");
    }

    #[test]
    fn deleting_the_last_chapter_also_collects_the_emptied_volume() {
        let (_dir, mut store, root) = setup();
        let (work, chapters) = book(&mut store);
        reconcile(&mut store, &root, work, true);
        for chapter in &chapters {
            store.soft_delete_node(*chapter).unwrap();
        }
        let plan = reconcile(&mut store, &root, work, true);

        assert_eq!(plan.actions.len(), 2, "两章两份都收掉：{:?}", plan.actions);
        assert!(!root.join(format!("长夜-{work}/001-第一卷")).exists(), "空掉的卷目录也要收掉");
        assert!(store.mirror_state(work).unwrap().is_empty());
    }

    #[test]
    fn a_body_change_rewrites_only_that_one_file() {
        let (_dir, mut store, root) = setup();
        let (work, chapters) = book(&mut store);
        reconcile(&mut store, &root, work, true);

        store.write_body(chapters[1], "第二章改过了。").unwrap();
        let plan = reconcile(&mut store, &root, work, false);

        assert_eq!(plan.actions.len(), 1, "只该写那一份：{:?}", plan.actions);
        let path = format!("长夜-{work}/001-第一卷/002-第二章.md");
        assert_eq!(read(&root, &path), "# 第二章\n\n第二章改过了。\n");
        assert!(plan.unchanged >= 1, "另一份没变，不该跟着重写");
    }
}
