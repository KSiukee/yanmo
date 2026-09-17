//! 逃生导出：**存不下去时，把手上这份正文写到哪**——只算落点，不写文件。
//!
//! # 为什么单独一份
//!
//! 它是"故障时还能不能救回字"那一件事的**路径策略**，与数据句柄的变化理由不同
//! （那边管数据目录、单实例、搬迁、会话与退出闸门）。分出来之后，"候选顺序"这条
//! 最要紧的取舍可以单独读、单独测（见文件末尾）。
//!
//! # 顺序为什么是这样（评审：中等 20）
//!
//! "存不下去"的常见原因正是**盘满、目录只读、介质写保护、UNC 断开**——逃生通道要是与故障
//! 同源（默认就写在数据目录里），最需要它的时候恰好也用不了。所以顺序是：
//!
//! ① 系统临时目录（通常另一块盘 / 另一个卷）→ ② 作者主目录 → ③ 数据目录（最后兜底）。
//! 调用方依次**真写**，第一个成功的就把实际落点报给界面。
//!
//! # 纪律
//!
//! - 命令层**不接受路径参数**：落点由这里算，界面无从指定别处；
//! - 这里只算路径，写盘交给调用方（原子写在核心，见 `yanmo_core::atomic`）。

use std::path::{Path, PathBuf};

/// 逃生导出目录（关窗存不下去时，把手上这份正文原子写到这里）。
pub const ESCAPE_DIR: &str = "escape";
/// 系统临时目录 / 主目录下的逃生文件夹名（语言无关，跟导出目录同一个语言）。
pub const ESCAPE_ROOT: &str = "YanmoEscape";

/// 数据目录里的逃生落点（库文件旁边）。
pub fn dir(db_path: &Path) -> PathBuf {
    db_path.parent().unwrap_or_else(|| Path::new(".")).join(ESCAPE_DIR)
}

/// 逃生导出的候选落点，**按"最不容易与故障同源"排序**（见文件头）。
pub fn candidates(db_path: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let push = |out: &mut Vec<PathBuf>, dir: PathBuf| {
        if !out.contains(&dir) {
            out.push(dir);
        }
    };
    push(&mut out, std::env::temp_dir().join(ESCAPE_ROOT));
    if let Some(home) = yanmo_core::paths::home_dir() {
        push(&mut out, home.join(ESCAPE_ROOT));
    }
    push(&mut out, dir(db_path));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-09-15 代码质量评审：中等 20——"存不下去"的常见原因正是盘满 / 只读 / 写保护，
    /// 逃生通道与故障同源（就在数据目录里）等于最需要它时用不了。数据目录只能兜底。
    #[test]
    fn the_data_directory_is_never_the_first_candidate() {
        let db = std::env::temp_dir().join("yanmo-escape-test").join("yanmo.db");
        let candidates = candidates(&db);

        assert!(candidates.len() >= 2, "至少要有临时目录与数据目录两站：{candidates:?}");
        assert_eq!(
            candidates.first().unwrap(),
            &std::env::temp_dir().join(ESCAPE_ROOT),
            "第一站必须是系统临时目录（通常另一块盘）"
        );
        assert_eq!(candidates.last().unwrap(), &dir(&db), "数据目录只能垫底");
        assert!(
            !candidates.contains(&db.parent().unwrap().to_path_buf()),
            "候选是子目录，不是库文件所在的那个目录本身"
        );
    }

    #[test]
    fn the_escape_directory_sits_next_to_the_database() {
        let db = Path::new("/data/yanmo.db");
        assert_eq!(dir(db), Path::new("/data").join(ESCAPE_DIR));
    }
}
