//! 作者信息命令：投稿时随稿子走的那点身份（笔名 / 真名 / 联系方式 / 文学简历）。
//!
//! 规则全在核心（`store::author`：稀疏合并、坏记录当没设过、留痕不抄取值）：
//! 这里只做参数转换 + 把**落定后的那一份**递回去（界面显示的永远是库里那份）。
//!
//! 一条口径：**敏感项默认不写进稿件**——身份证号与银行账号存在这一层是为了"投稿那家要时填一次"，
//! 真正写不写进文件由 [`yanmo_core::store::Store::author_block`] 那个开关决定。

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::model::{Author, ResolvedAuthor};
use yanmo_core::store::Store;

/// 读出来给人用的那一份（没设过的栏是空串）。
#[derive(Debug, Serialize)]
pub struct AuthorDto {
    pub pen_name: String,
    pub real_name: String,
    pub phone: String,
    pub email: String,
    pub address: String,
    pub postcode: String,
    pub bio: String,
    /// 敏感：身份证号（**默认不写进稿件**）
    pub id_number: String,
    /// 敏感：银行账号与开户行（**默认不写进稿件**）
    pub bank: String,
    /// 投稿时附不附作者简介（默认不附）
    pub attach: bool,
    /// 附的时候带不带敏感两项（默认不带）
    pub with_sensitive: bool,
}

impl From<ResolvedAuthor> for AuthorDto {
    fn from(value: ResolvedAuthor) -> Self {
        Self {
            pen_name: value.pen_name,
            real_name: value.real_name,
            phone: value.phone,
            email: value.email,
            address: value.address,
            postcode: value.postcode,
            bio: value.bio,
            id_number: value.id_number,
            bank: value.bank,
            attach: value.attach,
            with_sensitive: value.with_sensitive,
        }
    }
}

/// 要改的栏：**只写传进来的**，没传的保持原样（`Some("")` = 清空那一栏）。
#[derive(Debug, Default, Deserialize)]
pub struct AuthorPatch {
    pub pen_name: Option<String>,
    pub real_name: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub address: Option<String>,
    pub postcode: Option<String>,
    pub bio: Option<String>,
    pub id_number: Option<String>,
    pub bank: Option<String>,
    pub attach: Option<bool>,
    pub with_sensitive: Option<bool>,
}

impl From<AuthorPatch> for Author {
    fn from(patch: AuthorPatch) -> Self {
        Self {
            pen_name: patch.pen_name,
            real_name: patch.real_name,
            phone: patch.phone,
            email: patch.email,
            address: patch.address,
            postcode: patch.postcode,
            bio: patch.bio,
            id_number: patch.id_number,
            bank: patch.bank,
            attach: patch.attach,
            with_sensitive: patch.with_sensitive,
        }
    }
}

/// 读作者信息（没设过就是一份空的）。
#[tauri::command]
pub fn author_read(data: State<'_, AppData>) -> Result<AuthorDto, ApiError> {
    data.with_store(|store: &mut Store| Ok(store.author()?.into()))
}

/// 改作者信息：稀疏合并，写完**回读**（界面显示的永远是库里那份）。
#[tauri::command(rename_all = "snake_case")]
pub fn author_write(
    data: State<'_, AppData>,
    patch: AuthorPatch,
) -> Result<AuthorDto, ApiError> {
    data.with_store(|store: &mut Store| Ok(store.set_author(&patch.into())?.into()))
}

/// 清空（回到"没设过"）。
#[tauri::command]
pub fn author_reset(data: State<'_, AppData>) -> Result<AuthorDto, ApiError> {
    data.with_store(|store: &mut Store| Ok(store.reset_author()?.into()))
}
