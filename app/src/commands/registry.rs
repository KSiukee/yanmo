//! 命令登记处：**壳上所有命令的唯一一张清单**。
//!
//! 单独成件的原因与入口是"薄入口"同一条：这张清单**每加一个功能就长一行**
//! （到 0.78.0 已经 159 条），留在入口文件里迟早把入口撑成上帝文件——它已经贴到
//! 400 行那条线上，再加一行就红。入口那边于是只剩一句"接上本模块的 `app_handlers`"。
//!
//! ⚠️ 这一份里**不许再出现那个字面路径**（`commands::` + 模块名 + 函数名）：
//! 边界的守卫是按 `commands::` 后面那一段认命令名的，写进注释会被当成"一条注册了
//! 但没人实现的命令"（0.78.0 搬过来时就踩了这一下，守卫当场报出来）。
//!
//! 边界的守卫（`app/tests/boundary.rs`）拿这张清单对两件事：
//! 「源码里声明了哪些命令」与「前端网关登记了哪些命令」——**一一对应，漏一个就红**。
//!
//! 两处讲究：
//!
//! - 路径写成 `crate::commands::...`：这一份不在 crate 根，写 `commands::...` 认不出来；
//! - 运行时泛型写死 `tauri::Wry`：与入口那个 Builder 是同一个（换运行时两处一起换）。

/// 全部命令的处理器（`generate_handler!` 展开出来的那个闭包），**外面包一层计时**。
///
/// # 为什么计时包在这里
///
/// 壳上的命令全是**同步**的：登记处这一对括号从参数解析一路框到回包序列化
/// （也就是"作者从点下去到看见反应"的那段等待）——这是唯一一处**不用改 170 条命令**
/// 就能量全的地方。超过 5 秒的那条会由看门狗线程留痕（日志 + 库里那本账），
/// 判据与现场在 [`crate::slowcmd`]，**只为诊断，一个字都不改行为**。
pub fn app_handlers() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
    timed(tauri::generate_handler![
        crate::commands::system::engine_info,
        crate::commands::system::diagnose_note,
        crate::commands::system::data_home,
        crate::commands::system::open_data_dir,
        crate::commands::system::exit_app,
        // 慢命令与卡顿的现场（守护之心"慢"那一半的读数）
        crate::commands::system::slow_report,
        crate::commands::system::ui_lag,
        // 加密备份包那一族（开关 / 口令 / 恢复码，见 commands/seal.rs）
        crate::commands::seal::seal_status,
        crate::commands::seal::seal_enable,
        crate::commands::seal::seal_disable,
        crate::commands::seal::seal_unlock,
        crate::commands::seal::seal_lock,
        crate::commands::editor::open_editor_target,
        crate::commands::editor::open_chapter,
        crate::commands::editor::open_work_target,
        crate::commands::editor::create_chapter,
        crate::commands::editor::chapter_neighbors,
        crate::commands::editor::save_cursor,
        crate::commands::editor::save_body,
        crate::commands::editor::set_node_summary,
        crate::commands::editor::body_fingerprint,
        crate::commands::session::ui_alive,
        crate::commands::editor::emergency_snapshot,
        crate::commands::session::session_report,
        crate::commands::session::arm_exit_gate,
        crate::commands::session::ack_close_request,
        crate::commands::session::close_session,
        crate::commands::sop::sop_read,
        crate::commands::sop::sop_write,
        crate::commands::sop::sop_reset,
        crate::commands::sop::sop_mech_chapter,
        crate::commands::sop::sop_mech_book,
        crate::commands::session::abandon_session,
        crate::commands::editor::escape_export,
        crate::commands::tree::tree_children,
        crate::commands::tree::tree_ancestors,
        crate::commands::tree::tree_create_node,
        crate::commands::tree::tree_rename_node,
        crate::commands::tree::tree_move_node,
        crate::commands::tree::tree_delete_node,
        crate::commands::tree::tree_volume_target,
        crate::commands::tree::tree_set_volume_target,
        crate::commands::volume::volume_plan,
        crate::commands::volume::volume_offer,
        crate::commands::volume::volume_close,
        crate::commands::volume::volume_dissolve,
        crate::commands::author::author_read,
        crate::commands::author::author_write,
        crate::commands::author::author_reset,
        crate::commands::appearance::appearance_read,
        crate::commands::appearance::appearance_write,
        crate::commands::appearance::appearance_reset,
        crate::commands::question::question_drafts,
        crate::commands::question::question_sync,
        crate::commands::question::question_board,
        crate::commands::question_push::question_push,
        crate::commands::question::question_ask,
        crate::commands::question::question_answer,
        crate::commands::question::question_land_answer,
        crate::commands::question::question_apply_round,
        crate::commands::question::question_defer,
        crate::commands::question::question_discard,
        crate::commands::question::question_praise,
        crate::commands::question::question_mute_class,
        crate::commands::question::question_retrieve,
        crate::commands::question::question_unmute_class,
        crate::commands::question::question_undefer,
        crate::commands::question::question_inspire,
        crate::commands::question::question_mute_source,
        crate::commands::question::question_unmute_source,
        crate::commands::question::question_requeue_due,
        crate::commands::fragment::fragment_add,
        crate::commands::fragment::fragment_board,
        crate::commands::fragment::fragment_delete,
        crate::commands::fragment::fragment_restore,
        crate::commands::inspire::inspire_board,
        crate::commands::inspire::inspire_adopt,
        crate::commands::inspire::inspire_discard,
        crate::commands::inspire::inspire_collisions,
        crate::commands::inspire::inspire_collide,
        crate::commands::inspire::inspire_skip_collision,
        crate::commands::inspire::inspire_unskip_collision,
        crate::commands::entity::entity_list,
        crate::commands::entity::entity_create,
        crate::commands::entity::entity_update,
        crate::commands::entity::entity_delete,
        crate::commands::outline::outline_rows,
        crate::commands::outline::outline_scan,
        crate::commands::outline::outline_dismiss,
        crate::commands::outline::outline_undismiss,
        crate::commands::outline::outline_clear_dismissed,
        crate::commands::outline::outline_paste_cells,
        crate::commands::outline::outline_set_cast,
        // 章长：这一本书每一章多少字、哪一章偏长（只读；「就这样写」走既有的 outline_dismiss）
        crate::commands::chapter_length::chapter_lengths,
        crate::commands::outline::outline_actuals,
        crate::commands::outline::outline_align_cast,
        crate::commands::outline::outline_align_undo,
        // 五线推进度：这一本书每一段标了哪几条线（读 / 整份覆盖地写）
        crate::commands::lines::node_lines,
        crate::commands::lines::node_lines_set,
        crate::commands::foreshadow::foreshadow_list,
        crate::commands::foreshadow::foreshadow_create,
        crate::commands::foreshadow::foreshadow_update,
        crate::commands::foreshadow::foreshadow_move,
        crate::commands::foreshadow::foreshadow_delete,
        crate::commands::fragment::fragment_update,
        crate::commands::scene::save_node_fields,
        crate::commands::writing::writing_today,
        crate::commands::writing::writing_overview,
        crate::commands::typeset::typeset_rules,
        crate::commands::typeset::typeset_scan,
        crate::commands::typeset::typeset_apply,
        crate::commands::snapshot::snapshot_list,
        crate::commands::snapshot::snapshot_diff,
        crate::commands::snapshot::snapshot_keep,
        crate::commands::snapshot::snapshot_drop,
        crate::commands::snapshot::snapshot_restore,
        crate::commands::work::list_shelf,
        crate::commands::work::work_brief,
        crate::commands::work::create_work,
        crate::commands::work::rename_work,
        crate::commands::work::set_work_language,
        crate::commands::work::set_work_summary,
        crate::commands::work::set_work_target_words,
        crate::commands::work::work_storyline,
        crate::commands::work::work_set_storyline,
        crate::commands::work::naming_rewrite_preview,
        crate::commands::work::naming_rewrite_apply,
        crate::commands::backup::backup_status,
        crate::commands::backup::backup_config_write,
        crate::commands::backup::backup_now,
        crate::commands::restore::backup_restore_sources,
        crate::commands::restore::backup_restore_preview,
        crate::commands::restore::backup_restore_apply,
        crate::commands::restore::backup_restore_pick,
        crate::commands::location::data_location_info,
        crate::commands::location::data_location_confirm,
        crate::commands::location::data_location_pick,
        crate::commands::location::data_location_move,
        crate::commands::location::data_location_cancel,
        crate::commands::restore::backup_restore_unlock,
        crate::commands::mirror::mirror_status,
        crate::commands::mirror::mirror_set_enabled,
        crate::commands::mirror::mirror_sync_now,
        crate::commands::mirror::mirror_open_folder,
        crate::commands::mirror::mirror_resolve,
        crate::commands::work::delete_work,
        crate::commands::work::export_work,
        crate::commands::compile::compile_presets,
        crate::commands::compile::compile_preview,
        crate::commands::compile::compile_work,
        crate::commands::compile::compile_open_folder,
        crate::commands::compile::compile_collection,
        crate::commands::compile::compile_collection_open_folder,
        crate::commands::trash::list_trash,
        crate::commands::trash::restore_work,
        crate::commands::trash::restore_preview,
        crate::commands::trash::restore_node,
        crate::commands::trash::purge_node,
        crate::commands::trash::purge_work,
        crate::commands::trash::empty_trash,
        crate::commands::manuscript::manuscript_board,
        crate::commands::manuscript::manuscript_detail,
        crate::commands::manuscript::manuscript_verdict,
        crate::commands::manuscript::manuscript_submit,
        crate::commands::manuscript::manuscript_move,
        crate::commands::manuscript::manuscript_update_submission,
        crate::commands::manuscript::manuscript_delete_submission,
        crate::commands::collection::collection_view,
        crate::commands::collection::collection_add,
        crate::commands::collection::collection_remove,
        crate::commands::collection::collection_reorder,
        crate::commands::collection::collection_set_target,
        crate::commands::collection::collection_create,
        crate::commands::collection::collection_loose_articles,
        crate::commands::collection::collection_of,
        // 检索：正文里找一句话（命中带书名与片段，见 commands/search.rs）
        crate::commands::search::search_text,
    ])
}

/// 给整张命令表包一层计时。
///
/// 为什么写成"收一个处理器、返回一个处理器"：`generate_handler!` 展开出来的是一个闭包，
/// 直接 `let inner = ...` 编译器认不出它的签名（没有期待的类型）；当成实参递给这里，
/// 签名由 `F` 这个约束定住，才推得出来。
///
/// 计时只用来看"哪条命令让作者等了很久"：进出这一对括号就是那段等待
///（同步命令在登记处这一层是**行内执行**的，`inner` 返回时它已经干完了）。
/// **只留痕，绝不因此改任何行为**（判据与现场见 [`crate::slowcmd`]）。
fn timed<F>(
    inner: F,
) -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static
where
    F: Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static,
{
    move |invoke: tauri::ipc::Invoke<tauri::Wry>| {
        let command = invoke.message.command().to_string();
        // 守卫：离开这段就结账（连命令 panic 展开也收得干净，见 `slowcmd_wire::Timer`）
        let _timer = crate::slowcmd_wire::start(&command);
        inner(invoke)
    }
}
