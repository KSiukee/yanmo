//! 编辑类命令：**边写边存**与**关窗不放行**需要的那几个动作。
//!
//! 分工是刻意的：
//! - **击键留在壳内**（编辑器自己处理），只有"该落盘了 / 该核对一下 / 该关窗了"这些
//!   **低频动作**才过边界；
//! - 落盘用**内容指纹**判定，同一份内容重复提交不会产生任何写入；
//! - 读回校验只回传**指纹**，不把整章正文再搬一遍；
//! - 关窗走**闸门**：界面先落盘，存不下去就别想走（重试 / 导出逃生 / 仍然退出）。

use serde::Serialize;
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::model::SceneFields;
use yanmo_core::store::{EditorCursor, EditorTarget, Store};

/// 打开编辑器时拿到的一章。
#[derive(Debug, Serialize)]
pub struct EditorSnapshot {
    pub work_id: i64,
    pub node_id: i64,
    /// 章名 / 篇名
    pub title: String,
    /// 正文（纯文本，段落间空行分隔）
    pub body: String,
    /// 三个字数口径一起给：界面按作者选的那个显示（切换时不必再跑一趟核心）
    pub char_count: i64,
    pub chars_no_punct: i64,
    pub word_count: i64,
    /// 作品语言（`zh` / `en` / `ja`）——字数默认口径跟它走
    pub work_language: String,
    /// **落定后**的字数口径（作者选过就是它，没选过就是作品语言的默认）：
    /// 界面直接照着取数，不必自己再维护一份"语言 → 口径"的对照表
    pub word_caliber: &'static str,
    /// 库里这份正文的内容指纹；从未写过时为空串
    pub fingerprint: String,
    /// 上次读到哪了（**只有记的正是这一章时才有**）
    pub cursor: Option<CursorDto>,
    /// 这一章的"一句话"（作者手填，空串＝没写过）：投稿包的大纲要按阅读顺序取它
    pub summary: String,
    /// 打开的是**场景卡**时的那四格（视角 / 目标 / 冲突 / 结果）；别的节点是 `null`。
    ///
    /// 四格随快照一起给：界面上那一小张表单不该为它再跑一趟（与"章纲一句话"同一处出口）。
    pub scene: Option<SceneFields>,
}

/// 光标与滚动位置。
#[derive(Debug, Serialize)]
pub struct CursorDto {
    pub anchor: i64,
    pub head: i64,
    pub scroll_top: i64,
}

/// 导航用的章节条目。
#[derive(Debug, Serialize)]
pub struct ChapterSummaryDto {
    pub id: i64,
    pub title: String,
    pub word_count: i64,
}

/// 当前章的邻居与位置。
#[derive(Debug, Serialize)]
pub struct NeighborsDto {
    pub previous: Option<ChapterSummaryDto>,
    pub next: Option<ChapterSummaryDto>,
    pub index: i64,
    pub total: i64,
}

/// 组装一章的快照（打开与切换走同一条路，免得两处各写一份）。
fn snapshot_of(store: &Store, target: EditorTarget) -> yanmo_core::Result<EditorSnapshot> {
    let (body, stats) = store.read_body_with_stats(target.node_id)?;
    // 口径由核心落定（作者选过 → 它；没选过 → 作品语言的默认）：规则只写在核心一处
    let language = store.get_work(target.work_id)?.language;
    let caliber = store.word_caliber(target.work_id)?;
    // 场景卡才有那四格：先问一句"是不是"（**不用异常当控制流**）
    // 凡承载正文的节点都有四格（章也能直接填）；卷没有
    let scene = if store.has_fields(target.node_id)? {
        Some(store.scene_fields(target.node_id)?)
    } else {
        None
    };
    Ok(EditorSnapshot {
        work_id: target.work_id,
        node_id: target.node_id,
        title: target.title,
        summary: store.node_summary(target.node_id)?,
        scene,
        fingerprint: store.body_fingerprint(target.node_id)?,
        cursor: store.load_cursor(target.node_id)?.map(|c| CursorDto {
            anchor: c.anchor,
            head: c.head,
            scroll_top: c.scroll_top,
        }),
        body,
        char_count: stats.char_count,
        chars_no_punct: stats.chars_no_punct,
        word_count: stats.word_count,
        work_language: language.as_str().to_string(),
        word_caliber: caliber.as_str(),
    })
}

/// 一次落盘的回执。
#[derive(Debug, Serialize)]
pub struct SaveAck {
    pub char_count: i64,
    pub chars_no_punct: i64,
    pub word_count: i64,
    /// 这次落盘**提交的那份正文**的指纹（不是从库里读回来的值）。
    ///
    /// 界面拿它当"我以为库里现在是这个"的比对基准，之后由 `body_fingerprint` 轮询核对
    /// 库里真的是不是它——所以这里必须给手上这份的哈希，读回来的值反而会让校验失去意义
    /// （2026-09-15 代码质量评审：轻微 9）。
    pub fingerprint: String,
}

/// 上一次会话的交代。
#[derive(Debug, Serialize)]
pub struct SessionNotice {
    /// 上次没有正常退出（被杀 / 崩溃）
    pub unclean: bool,
    /// 崩溃前正在编辑的节点
    pub last_node_id: Option<i64>,
    /// 上次活动时间（unix 毫秒）
    pub last_seen_at: Option<i64>,
}

/// 退出收尾的回执。
#[derive(Debug, Serialize)]
pub struct CloseAck {
    /// 是否真的留了关窗快照（内容没变就不留，避免堆垃圾）
    pub snapshot_written: bool,
}

/// 逃生导出的结果。
#[derive(Debug, Serialize)]
pub struct EscapeAck {
    /// 导出到的完整路径（界面只展示，不碰文件系统）
    pub path: String,
}

/// 取当前该编辑的一章（若上次是被杀，优先回到崩溃前那一章）。
#[tauri::command]
pub fn open_editor_target(data: State<'_, AppData>) -> Result<EditorSnapshot, ApiError> {
    crate::acceptance::note_command("open_editor_target");
    let preferred = data.session().last_node_id;
    data.with_store(|store| {
        let target = store.ensure_editor_target_preferring(preferred)?;
        // 打开就记下是哪一章：万一还没写一个字就被杀，重开也能回到原位
        store.note_open_node(target.node_id)?;
        sweep_question_requeues(store, target.work_id);
        snapshot_of(store, target)
    })
}

/// 顺手把**条件已经满足**的延后放回候选池（打开作品 / 打开一章 / 建完一章时各喊一声）。
///
/// 为什么挂在这儿：这三个都是**低频时机**（不是击键那条路），也正是"时机到了"的语义现场——
/// 开新章意味着"写到某一章"那条条件可能刚刚满足。它失败不该连累开章，但也不许静默：
/// 记进体检日志（[`crate::diagnose`]），验收时看得到。
fn sweep_question_requeues(store: &mut Store, work_id: i64) {
    if let Err(error) = store.requeue_due_questions(work_id, yanmo_core::time::now_millis(), "shell")
    {
        // 体检日志（开发者/验收看）：用英文，免得被"界面文案回潮"守卫当成界面文案
        crate::diagnose::note(&format!("question requeue sweep failed: {error}"));
    }
}

/// 切到指定章节：这一章必须**真的能编辑**（不存在 / 已删除 / 不承载正文都会明确报错）。
///
/// 切章流程由界面负责"先落盘再切"；这里只保证"切得对"。
#[tauri::command(rename_all = "snake_case")]
pub fn open_chapter(data: State<'_, AppData>, node_id: i64) -> Result<EditorSnapshot, ApiError> {
    data.with_store(|store| {
        let target = store.editor_target(node_id)?;
        store.note_open_node(target.node_id)?;
        sweep_question_requeues(store, target.work_id);
        snapshot_of(store, target)
    })
}

/// 切到某一本书：落点是**这本书上次写的那一章**（记不起来了就给它的第一章）。
///
/// "换书"与"换章"在界面上是同一套纪律（先落盘再换），所以这里也只管"落点对不对"。
#[tauri::command(rename_all = "snake_case")]
pub fn open_work_target(data: State<'_, AppData>, work_id: i64) -> Result<EditorSnapshot, ApiError> {
    data.with_store(|store| {
        let target = store.work_target(work_id)?;
        store.note_open_node(target.node_id)?;
        sweep_question_requeues(store, target.work_id);
        snapshot_of(store, target)
    })
}

/// 在当前章后面新建一章，并把它作为当前章返回（界面点完就能直接切过去）。
///
/// **标题留空 = 由核心按同层序号取名**——默认名只有核心那一份实现。
#[tauri::command(rename_all = "snake_case")]
pub fn create_chapter(
    data: State<'_, AppData>,
    node_id: i64,
    title: String,
) -> Result<EditorSnapshot, ApiError> {
    data.with_store(|store| {
        let created = store.add_chapter_after(node_id, yanmo_core::model::NodeKind::Chapter, &title)?;
        let target = store.editor_target(created)?;
        store.note_open_node(target.node_id)?;
        sweep_question_requeues(store, target.work_id);
        snapshot_of(store, target)
    })
}

/// 上一章 / 下一章（按阅读顺序，跨卷）。
#[tauri::command(rename_all = "snake_case")]
pub fn chapter_neighbors(data: State<'_, AppData>, node_id: i64) -> Result<NeighborsDto, ApiError> {
    crate::acceptance::note_command("chapter_neighbors");
    data.with_store(|store| {
        let neighbors = store.chapter_neighbors(node_id)?;
        let to_dto = |chapter: yanmo_core::store::ChapterSummary| ChapterSummaryDto {
            id: chapter.id,
            title: chapter.title,
            word_count: chapter.word_count,
        };
        Ok(NeighborsDto {
            previous: neighbors.previous.map(to_dto),
            next: neighbors.next.map(to_dto),
            index: neighbors.index,
            total: neighbors.total,
        })
    })
}

/// 记下"这一章读到哪了"（失焦 / 切章 / 关窗时调用，不跟着击键走）。
#[tauri::command(rename_all = "snake_case")]
pub fn save_cursor(
    data: State<'_, AppData>,
    node_id: i64,
    anchor: i64,
    head: i64,
    scroll_top: i64,
) -> Result<(), ApiError> {
    data.with_store(|store| {
        store.save_cursor(node_id, EditorCursor { anchor, head, scroll_top })
    })
}

/// 写当前章的"一句话"（投稿包的大纲要用它）。
///
/// **与正文分开存**：它不参与落盘防抖，也不进正文——作者写不写它，正文一个字都不变。
#[tauri::command(rename_all = "snake_case")]
pub fn set_node_summary(
    data: State<'_, AppData>,
    node_id: i64,
    summary: String,
) -> Result<(), ApiError> {
    data.with_store(|store: &mut Store| store.set_node_summary(node_id, &summary))
}

/// 落盘正文（防抖后调用）。内容没变时核心直接返回，不写库、不记日志。
///
/// `tz_offset_minutes` 是作者本地时区相对 UTC 的偏移（东八区 = 480）：
/// 核心拿它算"这一笔算哪一天"，账本记在 `writing_days` 上（今日进度 / 码字日历）。
#[tauri::command(rename_all = "snake_case")]
pub fn save_body(
    data: State<'_, AppData>,
    node_id: i64,
    body: String,
    tz_offset_minutes: i32,
) -> Result<SaveAck, ApiError> {
    data.with_store(|store| {
        let stats = store.write_body_counted(node_id, &body, tz_offset_minutes)?;
        Ok(SaveAck {
            char_count: stats.char_count,
            chars_no_punct: stats.chars_no_punct,
            word_count: stats.word_count,
            fingerprint: yanmo_core::text::content_hash(&body),
        })
    })
}

/// 库里这份正文的指纹——**写后读回校验**用，避免每几秒搬运整章文本。
///
/// 界面每几秒就会调它一次，核心顺手把这次调用当作**心跳**（崩溃检测据此知道进程还活着）。
#[tauri::command(rename_all = "snake_case")]
pub fn body_fingerprint(data: State<'_, AppData>, node_id: i64) -> Result<String, ApiError> {
    data.with_store(|store| store.body_fingerprint(node_id))
}

/// 发现"库里的正文和手上这份对不上"时的抢救：先留快照，再把库改回手上的版本。
///
/// `tz_offset_minutes` 一并记进「每日码字」——抢救回来的字也是今天敲的。
#[tauri::command(rename_all = "snake_case")]
pub fn emergency_snapshot(
    data: State<'_, AppData>,
    node_id: i64,
    body: String,
    reason: String,
    tz_offset_minutes: i32,
) -> Result<SaveAck, ApiError> {
    data.with_store(|store| {
        let stats = store.emergency_snapshot(node_id, &body, &reason, tz_offset_minutes)?;
        Ok(SaveAck {
            char_count: stats.char_count,
            chars_no_punct: stats.chars_no_punct,
            word_count: stats.word_count,
            fingerprint: yanmo_core::text::content_hash(&body),
        })
    })
}

/// 上次会话的交代：界面启动时问一次，决定要不要提示"上次没有正常退出"。
#[tauri::command]
pub fn session_report(data: State<'_, AppData>) -> SessionNotice {
    let session = data.session();
    SessionNotice {
        unclean: session.unclean,
        last_node_id: session.last_node_id,
        last_seen_at: session.last_seen_at,
    }
}

/// 界面已就绪——从现在起关窗会先过闸门。
#[tauri::command]
pub fn arm_exit_gate(app: tauri::AppHandle, data: State<'_, AppData>) {
    crate::acceptance::note_command("ui.ready");
    data.arm_exit_gate();
    // 验收模式：界面刚就绪——把"冷启动到可用"这一刻量下来，写完报告壳自己退出
    crate::acceptance::ui_ready(&app);
}

/// 界面回话：收到关窗通知，开始处理（落盘，或弹"存不下去"的对话框）。
///
/// 壳据此停掉"界面是不是已经死了"的倒计时——**拦住关窗是界面故意的，而不是界面没反应**。
#[tauri::command]
pub fn ack_close_request(data: State<'_, AppData>) {
    data.exit_watch().acknowledge();
}

/// 正常退出前收尾：**留一份关窗快照 + 标记干净退出**。
#[tauri::command(rename_all = "snake_case")]
pub fn close_session(data: State<'_, AppData>, node_id: i64) -> Result<CloseAck, ApiError> {
    data.with_store(|store| {
        Ok(CloseAck {
            snapshot_written: store.end_session(node_id)?,
        })
    })
}

/// 用户选择"仍然退出"：只标记干净退出，不写快照（把选择权交还给人）。
#[tauri::command]
pub fn abandon_session(data: State<'_, AppData>) -> Result<(), ApiError> {
    data.with_store(|store| store.abandon_session())
}

/// 逃生导出：把手上这份正文**原子写**到第一个写得进去的落点，返回**实际落点**给界面展示。
///
/// 这是"存不下去"时唯一还能把字带走的通道——所以它自己必须是最可靠的那一段：
/// ① 同目录临时文件 + 落盘 + rename，绝不会留下半截文件；
/// ② **不与故障同源**：依次试系统临时目录 → 主目录 → 数据目录，哪个真写成了就用哪个，
///    把实际落点如实报给界面（评审：中等 20）。盘满 / 只读 / 写保护正是"存不下去"的
///    常见原因，只认数据目录就等于在同一个坑里再摔一次。
#[tauri::command(rename_all = "snake_case")]
pub fn escape_export(data: State<'_, AppData>, node_id: i64, body: String) -> Result<EscapeAck, ApiError> {
    let title = data.with_store(|store| store.node_title(node_id))?;
    let file_name = format!(
        "{}-{}.txt",
        yanmo_core::time::now_millis(),
        yanmo_core::atomic::safe_file_name(&title)
    );
    let path = write_escape(&data.escape_candidates(), &file_name, body.as_bytes())?;
    Ok(EscapeAck {
        path: path.display().to_string(),
    })
}

/// 依次试候选落点，返回**第一个真写成功**的完整路径；都写不进去才报错（带最后一次原因）。
///
/// 失败时给界面的永远是"最后一站为什么没成"——那个原因最接近"整条路都堵死了"的实情。
fn write_escape(
    candidates: &[std::path::PathBuf],
    file_name: &str,
    body: &[u8],
) -> Result<std::path::PathBuf, ApiError> {
    let mut failure: Option<ApiError> = None;
    for dir in candidates {
        let path = dir.join(file_name);
        match yanmo_core::atomic::write_atomic(&path, body) {
            Ok(()) => return Ok(path),
            Err(error) => {
                failure = Some(
                    ApiError::with(
                        "shell.export_write_failed",
                        [("path", path.display().to_string())],
                    )
                    .caused_by(&error),
                );
            }
        }
    }
    // 候选至少有一个（数据目录兜底）；"一个都没有"也按"写不进去"报，不 panic
    Err(failure.unwrap_or_else(|| {
        ApiError::with("shell.export_write_failed", [("path", String::new())])
            // i18n-allow-next-line: 开发者兜底（候选至少有一站，这一支构造上到不了），不进界面
            .caused_by("no escape candidate")
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 逃生导出：**一个落点写不进去就换下一个**，并把真正写成的那个路径回来。
    ///
    /// 2026-09-15 代码质量评审：中等 20——以前只有一个落点（数据目录），而"存不下去"的
    /// 常见原因正是那个盘满 / 只读 / 写保护，逃生通道于是与故障同源。
    #[test]
    fn escape_moves_on_to_the_next_candidate_when_one_fails() {
        let dir = tempfile::tempdir().unwrap();
        // 造一个必定建不出来的目录：父路径是个普通文件
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, b"x").unwrap();
        let dead = blocker.join("nope");
        let alive = dir.path().join("alive");

        let body = "手上的正文";
        let path = write_escape(&[dead, alive.clone()], "1-第一章.txt", body.as_bytes()).unwrap();

        assert_eq!(path, alive.join("1-第一章.txt"), "必须落到真正写得进去的那一站");
        assert_eq!(std::fs::read(&path).unwrap(), body.as_bytes());
    }

    #[test]
    fn escape_reports_the_last_failure_when_every_candidate_fails() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, b"x").unwrap();

        let error = write_escape(&[blocker.join("a"), blocker.join("b")], "1.txt", b"x")
            .expect_err("全都写不进去时必须报错，而不是默默成功");
        assert!(!error.code.is_empty(), "要有机器可读的码给界面查字典");
    }
}
