//! 正文背景图命令：**作者选图 → 壳复制进数据目录 → 库里只留一个文件名**。
//!
//! 分寸与别的域一致：命令只做"参数转换 + 转交核心 / 文件那一层"，
//! **不产界面文案**（窗口标题与筛选项由界面传进来），也不知道设置面板长什么样。
//!
//! # 顺序（有意如此）
//!
//! `background_pick`：① 系统对话框选图 → ② 壳把图复制进数据目录（校验不过就什么都不写）
//! → ③ **这一份成了**才落库（`set_background_image`）→ ④ 落库成功才删旧图。
//!
//! 任何一步失败，留在盘上的都是**上一次那一份**：不会出现"库里指着一张已经不存在的图"，
//! 也不会出现"新图复制进来了但库里还写着旧的"。
//!
//! # 界面拿不到路径
//!
//! 回给界面的只有：文件名、类型、字节数，以及**渲染用的那段 `data:` URL**。
//! 路径只活在壳里（`AppData::data_dir()`），所以界面拼不出数据目录之外的任何东西。

use serde::Serialize;
use tauri::State;

use crate::background;
use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::store::Store;

/// 换图 / 清图之后的回执。
#[derive(Debug, Serialize)]
pub struct BackgroundOutcome {
    /// 现在用的是哪个文件（null = 没有图）
    pub name: Option<String>,
    /// 界面要的画面（`data:` URL）；`null` = 现在没有图可画
    pub data_url: Option<String>,
    /// 这张图多少字节
    pub bytes: Option<u64>,
    /// 这张图是不是**还在**（图被作者从数据目录里删掉 / 搬走时是 false，界面回纯色）
    pub present: bool,
    /// 作者取消选图（或系统不给对话框）——`true` 时其余字段都保持原样
    pub cancelled: bool,
}

/// 现在的背景图：（文件名、能不能画、多少字节）。
///
/// 图不在了也**不报错**：`present = false` + 一句"回纯色"就是全部的处理，
/// 界面那边有对应的一句人话（见 `settings.background.image_gone`）。
fn snapshot(data: &AppData) -> Result<BackgroundOutcome, ApiError> {
    let name = data.with_store(|store: &mut Store| Ok(store.appearance(None)?.editor_bg_image))?;
    let Some(name) = name else {
        return Ok(BackgroundOutcome {
            name: None,
            data_url: None,
            bytes: None,
            present: false,
            cancelled: false,
        });
    };
    let read = background::read_data_url(&data.data_dir(), &name);
    let (data_url, bytes) = match read {
        Some((url, bytes)) => (Some(url), Some(bytes)),
        None => (None, None),
    };
    Ok(BackgroundOutcome {
        present: data_url.is_some(),
        name: Some(name),
        data_url,
        bytes,
        cancelled: false,
    })
}

/// 现在的背景图（界面打开设置面板时问一次）。
#[tauri::command]
pub fn background_image(data: State<'_, AppData>) -> Result<BackgroundOutcome, ApiError> {
    snapshot(&data)
}

/// 让作者挑一张图：复制进数据目录并立刻生效（**旧图在新图落库成功之后才删**）。
///
/// 窗口标题与筛选项由界面传进来——壳不产界面文案。
#[tauri::command(rename_all = "snake_case")]
pub fn background_pick(
    data: State<'_, AppData>,
    title: String,
    filter_label: String,
) -> Result<BackgroundOutcome, ApiError> {
    let Some(source) = pick_image_file(&title, &filter_label) else {
        // 取消：静默回去，什么都不做（与"选一个库文件"同一条）
        let mut outcome = snapshot(&data)?;
        outcome.cancelled = true;
        return Ok(outcome);
    };

    let data_dir = data.data_dir();
    // ① 先把新图复制进去（校验不过就一个字节都不写）
    let stored = background::store(&source, &data_dir)?;
    let size = stored.bytes;
    // ② 落库：**成了才算换图**（这一步失败就把刚复制进来的那份删掉，回到换之前的样子）
    let previous = data.with_store(|store: &mut Store| {
        let previous = store.appearance(None)?.editor_bg_image;
        store.set_background_image(None, Some(&stored.name))?;
        Ok(previous)
    });
    let previous = match previous {
        Ok(previous) => previous,
        Err(error) => {
            background::remove(&data_dir, &stored.name);
            return Err(error);
        }
    };
    // ③ 最后才删旧图（旧名字与新的同名时 `remove` 会把新图删掉——所以先比一下）
    if let Some(old) = previous {
        if old != stored.name {
            background::remove(&data_dir, &old);
        }
    }
    let mut outcome = snapshot(&data)?;
    // 回执里的字节数以**这一次搬进来的那一份**为准（读回失败时 snapshot 里是 None，
    // 但作者刚刚明明选成功了一张图——那一次的结果要如实说）
    outcome.bytes = Some(size);
    Ok(outcome)
}

/// 清掉背景图：库里那一格清空 + 数据目录里那几张删掉。
///
/// 顺序与换图相反（**先清库、再删文件**）：就算文件删不动（被占用 / 只读），
/// 界面那边也已经回到"没有图"，不会出现"库里还指着一张看不见的图"。
#[tauri::command]
pub fn background_clear(data: State<'_, AppData>) -> Result<BackgroundOutcome, ApiError> {
    data.with_store(|store: &mut Store| store.set_background_image(None, None))?;
    background::clear_dir(&data.data_dir());
    snapshot(&data)
}

/// 让作者挑一张图片文件。
///
/// 与"选一个库文件"（`commands::restore`）同一手法：**只在壳里调**，
/// 作者取消（或系统不给对话框）返回 `None`、静默回去。
#[cfg(windows)]
fn pick_image_file(title: &str, filter_label: &str) -> Option<std::path::PathBuf> {
    use windows_sys::Win32::UI::Controls::Dialogs::{
        GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain(std::iter::once(0)).collect() };
    // 过滤器是"成对的 `\0` 串，最后再来一个 `\0` 收尾"：标签\0*.png;*.jpg;...\0\0
    let mut filter = wide(filter_label);
    filter.extend(wide("*.png;*.jpg;*.jpeg;*.webp;*.gif"));
    filter.push(0);
    let title = wide(title);
    let default_ext = wide("png");
    let mut file = vec![0u16; 4096];

    let mut ofn: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    // 挂到当前前台窗口上，对话框才会出现在研墨上面而不是背后
    ofn.hwndOwner = unsafe { GetForegroundWindow() };
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrFile = file.as_mut_ptr();
    ofn.nMaxFile = file.len() as u32;
    ofn.lpstrTitle = title.as_ptr();
    ofn.lpstrDefExt = default_ext.as_ptr();
    // OFN_NOCHANGEDIR：不加的话对话框会把**进程的当前目录**改到选中文件那一层，必须拦住
    ofn.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;
    let ok = unsafe { GetOpenFileNameW(&mut ofn) };
    if ok == 0 {
        return None; // 作者取消
    }
    let end = file.iter().position(|unit| *unit == 0).unwrap_or(0);
    Some(std::path::PathBuf::from(String::from_utf16_lossy(&file[..end])))
}

/// 其余平台还没有发布形态：明确"没有这个能力"，不假装选到了。
#[cfg(not(windows))]
fn pick_image_file(_title: &str, _filter_label: &str) -> Option<std::path::PathBuf> {
    None
}
