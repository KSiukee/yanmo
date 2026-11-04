//! 系统类命令：引擎与环境信息（不含任何正文读写）。

use serde::Serialize;
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;

/// 引擎基本信息。
///
/// 存在的意义：验证「壳 → 核心」链路已打通（`yanmo-app` → `yanmo-core` / `yanmo-proto`）。
#[derive(Debug, Serialize)]
pub struct EngineInfo {
    /// 产品名
    pub name: &'static str,
    /// 引擎版本，来自 [`yanmo_core::engine_version`]
    pub version: &'static str,
    /// 本机协议版本，来自 [`yanmo_proto::PROTOCOL_VERSION`]
    pub protocol_version: u32,
    /// 数据格式版本，来自 [`yanmo_core::version::DATA_FORMAT_VERSION`]
    pub data_format_version: u32,
}

#[tauri::command]
/// 版本与引擎信息（界面底栏那条「v… · 库结构 …」用它）。
pub fn engine_info() -> EngineInfo {
    EngineInfo {
        // i18n-allow-next-line: 产品名（品牌），不翻译
        name: "研墨",
        version: yanmo_core::engine_version(),
        protocol_version: yanmo_proto::PROTOCOL_VERSION,
        data_format_version: yanmo_core::version::DATA_FORMAT_VERSION,
    }
}

/// 稿子落在哪里。
///
/// 路径由壳解析后**只读报告**给界面——用户有权知道自己的稿子在哪；
/// 但界面既不能指定它，也不能改写它（这里没有写入入口）。
#[derive(Debug, Serialize)]
pub struct DataHome {
    /// 数据库文件的绝对路径（仅供展示）
    pub path: String,
    /// 库内实际的数据结构版本
    pub schema_version: u32,
}

#[tauri::command]
/// 数据目录与它所在的卷（设置面板「稿子放在哪」与「关于」都读它）。
pub fn data_home(data: State<'_, AppData>) -> Result<DataHome, ApiError> {
    Ok(DataHome {
        path: data.db_path().display().to_string(),
        schema_version: data.schema_version()?,
    })
}

/// 在文件管理器中打开稿子所在的目录。
///
/// **不接受路径参数**：只打开壳自己持有的数据目录（界面无从指定别处）。
/// 「稿子到底在哪」是作者最常见的疑问，比让他去地址栏里手抄路径友好得多。
#[tauri::command(rename_all = "snake_case")]
pub fn open_data_dir(data: State<'_, AppData>) -> Result<(), ApiError> {
    let dir = data
        .db_path()
        .parent()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| {
            // detail 是给日志与排查用的技术说明（不是界面文案），所以写英文
            ApiError::new("shell.data_dir_unavailable").caused_by("data path has no parent directory")
        })?;
    crate::open_folder::open(&dir).map_err(|detail| {
        ApiError::with("shell.open_dir_failed", [("path", dir.display().to_string())])
            .caused_by(detail)
    })
}

/// 启动诊断：前端把它观察到的焦点与输入法事件递进来（**没开诊断时什么都不做**）。
///
/// 为什么要前端说话：只有页面里能看清"焦点到底在哪个元素上""输入法组字事件有没有来"，
/// 而这两件事正是那个 Win10 输入法毛病的分水岭。
#[tauri::command(rename_all = "snake_case")]
pub fn diagnose_note(text: String) {
    crate::diagnose::note(&text);
}

/// 一条命令的耗时现场（给设置里那块"运行诊断"看；**只是读数**）。
#[derive(Debug, Serialize)]
pub struct SlowCommandDto {
    pub command: String,
    pub ms: u64,
}

/// 库里记着的那一条（界面被重载过之后，壳里那份内存记录就没了，只有它还在）。
#[derive(Debug, Serialize)]
pub struct PersistedSlowCommandDto {
    pub command: String,
    pub ms: u64,
    /// 记下那一刻（unix 毫秒）
    pub at: i64,
}

/// 慢命令的一份读数：跑完的、还在飞的、以及库里那一条。
#[derive(Debug, Serialize)]
pub struct SlowReport {
    /// 这一趟会话里跑完的慢命令（**新的在前**）
    pub recent: Vec<SlowCommandDto>,
    /// **此刻还在飞**的命令（界面正等在哪一条上；慢的在前）
    pub inflight: Vec<SlowCommandDto>,
    /// 库里记着的最近一条（没有就是 null）
    pub last_in_db: Option<PersistedSlowCommandDto>,
}

/// 慢命令的现场（**只读报告**）：判据与留档在 [`crate::slowcmd`]。
///
/// 为什么界面能问：一条命令跑了十几秒之后，作者想知道"刚才卡在哪儿"——
/// 界面上那个读数（编辑器状态栏）只说"卡过"，完整清单要有个地方看。
#[tauri::command(rename_all = "snake_case")]
pub fn slow_report(data: State<'_, AppData>) -> SlowReport {
    let pairs = |command: String, ms: u64| SlowCommandDto { command, ms };
    SlowReport {
        recent: crate::slowcmd_wire::recent().into_iter().map(|s| pairs(s.command, s.ms)).collect(),
        inflight: crate::slowcmd_wire::inflight().into_iter().map(|s| pairs(s.command, s.ms)).collect(),
        last_in_db: data.live_slow_command().map(|slow| PersistedSlowCommandDto {
            command: slow.command,
            ms: slow.ms,
            at: slow.at,
        }),
    }
}

/// 界面自己量到的一次卡顿（1 秒定时器迟了多久）：**只留痕，什么都不做**。
///
/// 界面上那句读数由界面自己说（它当场就量到了，不必再往返一趟）；这一条是给事后查的：
/// 诊断日志里一句 + 库里一条（诊断日志多半没开，而"当时卡了多久"要能翻得到）。
#[tauri::command(rename_all = "snake_case")]
pub fn ui_lag(ms: u64, data: State<'_, AppData>) {
    // 英文：给开发者与体检看，不是界面文案
    crate::diagnose::note(&format!("ui stall: the main thread was blocked for about {ms}ms"));
    data.note_ui_lag_now(ms);
}

/// 系统里装了哪些字体族（**只读注册表**，不加载字体文件）。
///
/// 用途：排版里「更多字体…」那个可搜索列表。规则很克制：
/// - 只列**字体族名**：去掉 `(TrueType)`/`(OpenType)` 这类后缀，按 ` & ` 拆开
///   （注册表一条里常写两个别名，比如 `Microsoft YaHei & Microsoft YaHei UI`）；
/// - 去掉重名（中英文各一条很常见，保留先出现的那条）；
/// - 只读，不改任何东西；非 Windows 平台返回空数组。
#[tauri::command(rename_all = "snake_case")]
pub fn system_fonts() -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    #[cfg(windows)]
    {
        use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
        use winreg::RegKey;

        const KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts";
        for hive in [
            RegKey::predef(HKEY_LOCAL_MACHINE),
            RegKey::predef(HKEY_CURRENT_USER),
        ] {
            if let Ok(key) = hive.open_subkey_with_flags(KEY, KEY_READ) {
                for (value, _) in key.enum_values().flatten() {
                    names.extend(font_families_of(&value));
                }
            }
        }
    }
    names.sort_by_key(|name| name.to_lowercase());
    names.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    names
}

/// 一条注册表值名 → 一到两个**字体族名**（去掉字重/字形后缀）。
///
/// 例：`Microsoft YaHei & Microsoft YaHei UI (TrueType)` → `Microsoft YaHei`、`Microsoft YaHei UI`；
/// `Arial Bold Italic (TrueType)` → `Arial`（注册表把每个字形各列一条，界面上那是个菜单，
/// 不该出现"Arial Bold"和"Arial Bold Italic"这种按字重分的重复项）。
fn font_families_of(raw: &str) -> Vec<String> {
    let stripped = match raw.rfind(" (") {
        Some(index) => &raw[..index],
        None => raw,
    };
    stripped
        .split(" & ")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(family_name_of)
        .filter(|name| !name.is_empty())
        .collect()
}

/// 去掉结尾的字重 / 字形词（可能连着好几个，比如 `Bold Italic`）。
const STYLE_SUFFIXES: [&str; 14] = [
    "bold italic",
    "bold oblique",
    "semi bold",
    "semi bold italic",
    "demibold",
    "semibold",
    "bold",
    "italic",
    "oblique",
    "light",
    "regular",
    "medium",
    "black",
    "heavy",
];

fn family_name_of(part: &str) -> String {
    let mut name = part.trim().to_string();
    loop {
        let lower = name.to_lowercase();
        let mut trimmed = false;
        for suffix in STYLE_SUFFIXES {
            if lower.ends_with(suffix) {
                let keep = name.len() - suffix.len();
                name.truncate(keep);
                name = name.trim_end().to_string();
                trimmed = true;
                break;
            }
        }
        if !trimmed {
            break;
        }
    }
    name
}

/// 退出应用。
///
/// **只由退出闸门通过后调用**：关窗那一步已经先落盘、并留好了关窗快照；
/// 这里不再做任何判断，免得两处逻辑各说各话。顺手告诉兜底时钟"退出已开始"。
#[tauri::command]
pub fn exit_app(app: tauri::AppHandle, data: State<'_, AppData>) {
    // 收工前给变过的模块补一份镜像：**失败也不挡退出**（退出比镜子重要）。
    if let Ok(module_data) = yanmo_core::store::ModuleData::open(
        yanmo_core::store::ModuleData::path_beside(&data.data_dir().join("yanmo.db")),
    ) {
        let _ = module_data.refresh_mirrors_changed();
    }
    data.exit_watch().mark_exiting();
    app.exit(0);
}

#[cfg(test)]
mod tests {
    use super::font_families_of;

    #[test]
    fn registry_names_split_into_font_families() {
        assert_eq!(
            font_families_of("Microsoft YaHei & Microsoft YaHei UI (TrueType)"),
            vec!["Microsoft YaHei".to_string(), "Microsoft YaHei UI".to_string()]
        );
        assert_eq!(
            font_families_of("宋体 & 新宋体 (TrueType)"),
            vec!["宋体".to_string(), "新宋体".to_string()]
        );
        // 没有括号、没有别名的那种也不该被丢掉
        assert_eq!(font_families_of("SimSun"), vec!["SimSun".to_string()]);
        // 注册表把每个字重各列一条：界面上只留族名
        assert_eq!(font_families_of("Arial Bold Italic (TrueType)"), vec!["Arial".to_string()]);
        assert_eq!(font_families_of("Arial Bold (TrueType)"), vec!["Arial".to_string()]);
        assert_eq!(font_families_of("Segoe UI Light (TrueType)"), vec!["Segoe UI".to_string()]);
        assert_eq!(font_families_of("等线 Light (TrueType)"), vec!["等线".to_string()]);
    }
}
