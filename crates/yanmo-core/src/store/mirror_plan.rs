//! 磁盘 `.md` 镜像的**决策**：该有哪些文件、哪些要写 / 改名 / 删、哪些被人改过不能动。
//!
//! 纯函数 + 纯数据，**不碰文件系统、不碰数据库**：磁盘那边是什么样由调用方探（[`DiskState`]）。
//! 这样"哪些文件是我们写的"这条判断才可能被单测穷举（见 `tests/mirror.rs`）。
//!
//! # 两把指纹
//!
//! 每个文件记两份：**正文指纹**（库这边的内容）与**文件指纹**（我们写下去的字节）。
//! 前者让"库变了没有"**不必读正文**就能判断；后者让"磁盘上那份还是不是我们写的"在落盘前
//! 就能判——**不是我们写的就一个字都不碰**。
//!
//! # 宁可留着，也不覆盖
//!
//! 凡是拿不准的（文件被人改过、目标路径上有一份不是我们写的），一律记一条 [`MirrorConflict`]
//! 然后**跳过**：库里那份还在、作者改的那份也还在，谁都没丢——等作者自己定夺。
//!
//! # 动作的顺序是契约
//!
//! [`MirrorPlan::actions`] 一律按"**先腾位置（改名）→ 再写 → 最后收残留**"排好，
//! 落盘那一层照着顺序执行即可；收残留还会避开这一轮刚写 / 刚改名过去的位置，
//! 否则会把刚写好的文件当孤儿删掉（那个坑在 `tests/mirror.rs` 里钉着）。

use std::collections::{HashMap, HashSet};

/// 镜像里的一个文件（`relative_path` 相对**镜像根**）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorFile {
    pub node_id: i64,
    pub relative_path: String,
    /// 库里正文的指纹（**不读正文**就能判"库这边变了没有"）
    pub body_hash: String,
    /// 文件的字节（标题那一行 + 正文）
    pub content: Vec<u8>,
    /// 文件字节的指纹（判"磁盘上那份还是不是我们写的"）
    pub file_hash: String,
}

/// 账本里的一行：某个节点上次写进镜像的落点与两把指纹。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorEntry {
    pub node_id: i64,
    pub relative_path: String,
    pub body_hash: String,
    pub file_hash: String,
    pub size_bytes: i64,
    /// 已登记的外部改动：**不再碰它**，等作者定夺
    pub conflict: bool,
}

/// 这次对账之后要写进账本的一行（含冲突行）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorRecord {
    pub node_id: i64,
    pub relative_path: String,
    pub body_hash: String,
    pub file_hash: String,
    pub size_bytes: i64,
    pub conflict: bool,
}

/// 磁盘上某一份文件现在的状态——**由壳探测**（核心不碰文件系统）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskState {
    /// 文件不在
    Missing,
    /// 在，而且就是我们上次写下去的那一份（字节指纹对得上）
    Ours,
    /// 在，但**不是我们写的那一份**——作者用别的工具动过
    Foreign,
}

/// 一次对账要做的动作（**按 [`MirrorPlan::actions`] 给的顺序执行**）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirrorAction {
    /// 换路径（改名 / 移动）：旧文件还是我们写的，直接 `rename`
    Rename { node_id: i64, from: String, to: String },
    /// 写：新文件、内容变了、或文件被人删了
    Write { node_id: i64, relative_path: String },
    /// 收掉我们写的、这次不再需要的文件（章被删了 / 书没了）
    Remove { node_id: i64, relative_path: String },
}

/// 为什么动不了。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictReason {
    /// 我们写的那份被人改过
    EditedExternally,
    /// 想写的路径上有一份不是我们写的文件（不覆盖别人的东西）
    ForeignFile,
}

/// 一条动不了的记录（报告给作者看；合并 / 覆盖由下一步做）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorConflict {
    /// 换了路径的节点：冲突发生在旧路径上，这里仍是那个节点
    pub node_id: Option<i64>,
    pub relative_path: String,
    pub reason: ConflictReason,
}

/// 一次对账的完整结果：要做的动作 + 执行完该写进账本的行。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MirrorPlan {
    pub actions: Vec<MirrorAction>,
    /// 库与磁盘两边都没变、这次一个字都不用写的文件数
    pub unchanged: usize,
    /// 执行完之后账本的完整内容（含冲突行）
    pub records: Vec<MirrorRecord>,
    /// 要从账本里删掉的行（节点已不在、文件也已安全收掉）
    pub dropped: Vec<i64>,
    pub conflicts: Vec<MirrorConflict>,
}

impl MirrorPlan {
    /// 冲突的节点集合（报告与"这本书还没对上"的判断都用它）。
    pub fn conflicted_nodes(&self) -> HashSet<i64> {
        self.conflicts.iter().filter_map(|c| c.node_id).collect()
    }
}

/// 算一次对账计划。
///
/// `probe(path)` 由壳给：**只看那一个路径**的字节指纹对不对得上账（见 [`DiskState`]）。
///
/// `verify` = **全量核对**：每一份都要亲眼比过磁盘才算数。启动、空闲巡检、手动"立即同步"
/// 走这条；日常对账**不走**——不然作者每敲一下都要把整本书的 `.md` 重读一遍去比字节，
/// 那点 I/O 迟早会吃掉"边写边存"的顺滑。账与库一致时就不看磁盘，这是明写的取舍。
pub fn plan_mirror(
    desired: &[MirrorFile],
    previous: &[MirrorEntry],
    probe: impl Fn(&str) -> DiskState,
    verify: bool,
) -> MirrorPlan {
    let by_node: HashMap<i64, &MirrorEntry> = previous.iter().map(|e| (e.node_id, e)).collect();
    let mut plan = MirrorPlan::default();
    let mut renames: Vec<MirrorAction> = Vec::new();
    let mut writes: Vec<MirrorAction> = Vec::new();
    let mut removes: Vec<MirrorAction> = Vec::new();
    let mut conflict_paths: HashSet<String> = HashSet::new();

    for file in desired {
        // 内容（标题 + 正文）变了没有——**与路径无关**：改名那一支要单独问这句话
        let content_same = |prev: &MirrorEntry| {
            prev.body_hash == file.body_hash && prev.file_hash == file.file_hash
        };
        // 路径与内容都对上、且没登记过冲突：这一份这次一个字都不用写
        let unchanged_now = |prev: &MirrorEntry| {
            prev.relative_path == file.relative_path && content_same(prev) && !prev.conflict
        };

        if !verify && by_node.get(&file.node_id).is_some_and(|prev| unchanged_now(prev)) {
            plan.unchanged += 1;
            continue;
        }
        match by_node.get(&file.node_id) {
            None => match probe(&file.relative_path) {
                // 路径空着（或上一轮账被清过、那仍是我们写的）：写
                DiskState::Missing | DiskState::Ours => writes.push(MirrorAction::Write {
                    node_id: file.node_id,
                    relative_path: file.relative_path.clone(),
                }),
                DiskState::Foreign => plan.conflicts.push(MirrorConflict {
                    node_id: Some(file.node_id),
                    relative_path: file.relative_path.clone(),
                    reason: ConflictReason::ForeignFile,
                }),
            },
            Some(prev) if prev.relative_path == file.relative_path => {
                match probe(&file.relative_path) {
                    DiskState::Foreign => plan.conflicts.push(MirrorConflict {
                        node_id: Some(file.node_id),
                        relative_path: file.relative_path.clone(),
                        reason: ConflictReason::EditedExternally,
                    }),
                    DiskState::Missing => writes.push(MirrorAction::Write {
                        node_id: file.node_id,
                        relative_path: file.relative_path.clone(),
                    }),
                    DiskState::Ours => {
                        if unchanged_now(prev) || content_same(prev) {
                            // 后者是"账上挂着冲突、但文件已经变回我们要的内容"：
                            // **作者把它改回来了**，冲突当场解除，也不必再写一遍。
                            plan.unchanged += 1;
                        } else {
                            writes.push(MirrorAction::Write {
                                node_id: file.node_id,
                                relative_path: file.relative_path.clone(),
                            });
                        }
                    }
                }
            }
            Some(prev) => {
                // 换了路径：旧文件还是我们写的那份才敢动它
                let old = probe(&prev.relative_path);
                if old == DiskState::Foreign {
                    // 旧那份被人改过：**不动**（旧路径留一条冲突），新路径照写。
                    //
                    // 写自己的文件不丢任何东西：库里那份还在，作者改的那份也原样躺在旧路径上。
                    // 那份旧文件从此不在账上（账一行对一个节点），"启动时检测"会按目录扫出
                    // 这类没账的文件一并报给作者——在此之前，它是本层唯一会留在盘上的痕迹。
                    plan.conflicts.push(MirrorConflict {
                        node_id: Some(file.node_id),
                        relative_path: prev.relative_path.clone(),
                        reason: ConflictReason::EditedExternally,
                    });
                    writes.push(MirrorAction::Write {
                        node_id: file.node_id,
                        relative_path: file.relative_path.clone(),
                    });
                } else if probe(&file.relative_path) == DiskState::Foreign {
                    plan.conflicts.push(MirrorConflict {
                        node_id: Some(file.node_id),
                        relative_path: file.relative_path.clone(),
                        reason: ConflictReason::ForeignFile,
                    });
                } else if old == DiskState::Ours && probe(&file.relative_path) == DiskState::Missing {
                    // 干净改名：rename 保住磁盘上的历史（版本工具认得出这是同一个文件）
                    renames.push(MirrorAction::Rename {
                        node_id: file.node_id,
                        from: prev.relative_path.clone(),
                        to: file.relative_path.clone(),
                    });
                    if !content_same(prev) {
                        // 改名同时又改了内容：rename 之后再补一次写
                        writes.push(MirrorAction::Write {
                            node_id: file.node_id,
                            relative_path: file.relative_path.clone(),
                        });
                    }
                } else {
                    // 目标上是我们写的另一份（对应节点也要挪走）、或旧文件已经不在：直接写新路径
                    writes.push(MirrorAction::Write {
                        node_id: file.node_id,
                        relative_path: file.relative_path.clone(),
                    });
                    if old == DiskState::Ours {
                        removes.push(MirrorAction::Remove {
                            node_id: prev.node_id,
                            relative_path: prev.relative_path.clone(),
                        });
                    }
                }
            }
        }
    }

    // 账上有、这次不该有的：节点被删 / 书没了
    let wanted: HashSet<i64> = desired.iter().map(|f| f.node_id).collect();
    for prev in previous.iter().filter(|e| !wanted.contains(&e.node_id)) {
        match probe(&prev.relative_path) {
            DiskState::Ours => removes.push(MirrorAction::Remove {
                node_id: prev.node_id,
                relative_path: prev.relative_path.clone(),
            }),
            // 被人改过的：不删，账留着（每轮都还记得它，等作者定夺）
            DiskState::Foreign => plan.conflicts.push(MirrorConflict {
                node_id: Some(prev.node_id),
                relative_path: prev.relative_path.clone(),
                reason: ConflictReason::EditedExternally,
            }),
            DiskState::Missing => {}
        }
    }

    // **收残留必须避开这一轮要写 / 要改名过去的位置**：否则会把刚写好的文件删掉
    let keep: HashSet<String> = writes
        .iter()
        .chain(renames.iter())
        .map(|action| match action {
            MirrorAction::Write { relative_path, .. } => relative_path.clone(),
            MirrorAction::Rename { to, .. } => to.clone(),
            _ => String::new(),
        })
        .collect();
    removes.retain(|action| match action {
        MirrorAction::Remove { relative_path, .. } => !keep.contains(relative_path),
        _ => true,
    });

    plan.actions.extend(renames);
    plan.actions.extend(writes);
    plan.actions.extend(removes);
    plan.conflicts.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    conflict_paths.extend(plan.conflicts.iter().map(|c| c.relative_path.clone()));

    // 账本：每个该有的文件一行（冲突行也留，好让下一轮不必重扫整本正文）
    plan.records = desired
        .iter()
        .map(|file| MirrorRecord {
            node_id: file.node_id,
            relative_path: file.relative_path.clone(),
            body_hash: file.body_hash.clone(),
            file_hash: file.file_hash.clone(),
            size_bytes: file.content.len() as i64,
            conflict: conflict_paths.contains(&file.relative_path),
        })
        .collect();
    // 已经不存在的节点：动不了的把账原样留着，收干净的删账
    let wanted_conflicts: HashSet<i64> = plan.conflicted_nodes();
    for prev in previous.iter().filter(|e| !wanted.contains(&e.node_id)) {
        if wanted_conflicts.contains(&prev.node_id) {
            plan.records.push(MirrorRecord {
                node_id: prev.node_id,
                relative_path: prev.relative_path.clone(),
                body_hash: prev.body_hash.clone(),
                file_hash: prev.file_hash.clone(),
                size_bytes: prev.size_bytes,
                conflict: true,
            });
        } else {
            plan.dropped.push(prev.node_id);
        }
    }
    plan
}
