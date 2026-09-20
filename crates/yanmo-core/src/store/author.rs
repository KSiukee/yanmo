//! 作者信息的**存取**：一份全局记录（`settings` 的 `author` 键），稀疏合并、留痕。
//!
//! 与外观偏好（`store::appearance`）同一套做法，只有一条不同：
//! **留痕里不抄取值**——身份证号与银行账号不该进变更日志（日志是给排查看的，
//! 不需要为了排查把卡号抄一份）。日志只记"这次动了哪几栏"。
//!
//! 读写两条路的分寸也不同（与 appearance 同一条）：**写时严**（过长当场说清哪一栏）、
//! **读时宽**（坏 JSON / 认不出的字段当没设过，绝不因为一格坏数据把界面挡住）。

use rusqlite::{params, OptionalExtension};
use serde_json::json;

use super::Store;
use crate::error::{codes, Error, Result};
use crate::model::{Author, AuthorBlock, ResolvedAuthor};
use crate::time::now_millis;

/// 作者信息在 `settings` 里的键（**一份全局**：跟人，不跟书）。
const AUTHOR_KEY: &str = "author";

/// 每一栏的长度上限（自由文本那一栏放宽）。
const MAX_FIELD_CHARS: usize = 200;
const MAX_BIO_CHARS: usize = 2000;

impl Store {
    /// 读作者信息（没设过 / 坏记录 → 空的那一份）。
    pub fn author(&self) -> Result<ResolvedAuthor> {
        let raw: Option<String> = self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", params![AUTHOR_KEY], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(resolve(raw.as_deref().and_then(parse_sparse)))
    }

    /// 投稿出口用：**要写进稿件的作者简介**（没勾"附"、或什么都没填 → `None`）。
    pub fn author_block(&self) -> Result<Option<AuthorBlock>> {
        Ok(self.author()?.block())
    }

    /// 改作者信息：**只写传进来的那几栏**（没传的保持原样），回读落定后的那一份。
    pub fn set_author(&mut self, patch: &Author) -> Result<ResolvedAuthor> {
        let checked = checked(patch)?;
        let mut current = self.sparse_author()?;
        merge(&mut current, &checked);
        let touched = touched_fields(&checked);

        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES(?1, ?2, ?3)",
            params![
                AUTHOR_KEY,
                serde_json::to_string(&current).unwrap_or_else(|_| "{}".to_string()),
                now_millis()
            ],
        )?;
        // 留痕只记"动了哪几栏"，不抄取值（见文件头那条理由）
        Store::record_in(&self.device_id, &tx, "settings", 0, "set_author", json!({ "fields": touched }))?;
        tx.commit()?;
        Ok(resolve(Some(current)))
    }

    /// 清空（删掉那一行，回到"没设过"）。
    pub fn reset_author(&mut self) -> Result<ResolvedAuthor> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM settings WHERE key = ?1", params![AUTHOR_KEY])?;
        Store::record_in(&self.device_id, &tx, "settings", 0, "reset_author", json!({}))?;
        tx.commit()?;
        Ok(ResolvedAuthor::empty())
    }

    /// 库里那份**稀疏**记录（没设过 → 空的）。
    fn sparse_author(&self) -> Result<Author> {
        let raw: Option<String> = self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", params![AUTHOR_KEY], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(raw.as_deref().and_then(parse_sparse).unwrap_or_default())
    }
}

/// 坏 JSON = 没设过（与外观偏好同一条：宁可回到默认，也不拿坏数据挡住界面）。
fn parse_sparse(text: &str) -> Option<Author> {
    serde_json::from_str::<Author>(text).ok().map(tidy)
}

/// 逐栏收拾：收边、空的当"没设过"（`Some("")` 在这一层只表示"清空"）。
fn tidy(mut author: Author) -> Author {
    for field in [
        &mut author.pen_name,
        &mut author.real_name,
        &mut author.phone,
        &mut author.email,
        &mut author.address,
        &mut author.postcode,
        &mut author.bio,
        &mut author.id_number,
        &mut author.bank,
    ] {
        if let Some(text) = field {
            *text = text.trim().to_string();
        }
    }
    author
}

/// 写之前那一道：逐栏收边、过长当场说清是哪一栏。
fn checked(author: &Author) -> Result<Author> {
    let mut out = author.clone();
    for (name, field, max) in [
        ("pen_name", &mut out.pen_name, MAX_FIELD_CHARS),
        ("real_name", &mut out.real_name, MAX_FIELD_CHARS),
        ("phone", &mut out.phone, MAX_FIELD_CHARS),
        ("email", &mut out.email, MAX_FIELD_CHARS),
        ("address", &mut out.address, MAX_FIELD_CHARS),
        ("postcode", &mut out.postcode, MAX_FIELD_CHARS),
        ("bio", &mut out.bio, MAX_BIO_CHARS),
        ("id_number", &mut out.id_number, MAX_FIELD_CHARS),
        ("bank", &mut out.bank, MAX_FIELD_CHARS),
    ] {
        if let Some(text) = field {
            let trimmed = text.trim().to_string();
            if trimmed.chars().count() > max {
                return Err(Error::invalid_with(
                    codes::AUTHOR_TOO_LONG,
                    [("field", name.to_string()), ("max", max.to_string())],
                ));
            }
            *field = Some(trimmed);
        }
    }
    Ok(out)
}

/// 稀疏合并：patch 里 `Some` 的栏盖过去（`Some("")` = 清空），`None` 的不动。
fn merge(current: &mut Author, patch: &Author) {
    macro_rules! apply {
        ($($field:ident),* $(,)?) => {
            $( if patch.$field.is_some() { current.$field = patch.$field.clone(); } )*
        };
    }
    apply!(pen_name, real_name, phone, email, address, postcode, bio, id_number, bank, attach, with_sensitive);
}

/// 这次动了哪几栏（留痕用；**取值不进日志**）。
fn touched_fields(patch: &Author) -> Vec<&'static str> {
    let mut out = Vec::new();
    for (name, touched) in [
        ("pen_name", patch.pen_name.is_some()),
        ("real_name", patch.real_name.is_some()),
        ("phone", patch.phone.is_some()),
        ("email", patch.email.is_some()),
        ("address", patch.address.is_some()),
        ("postcode", patch.postcode.is_some()),
        ("bio", patch.bio.is_some()),
        ("id_number", patch.id_number.is_some()),
        ("bank", patch.bank.is_some()),
        ("attach", patch.attach.is_some()),
        ("with_sensitive", patch.with_sensitive.is_some()),
    ] {
        if touched {
            out.push(name);
        }
    }
    out
}

/// 稀疏 → 具体值（没设过的栏是空串）。
fn resolve(sparse: Option<Author>) -> ResolvedAuthor {
    let sparse = sparse.unwrap_or_default();
    let text = |value: Option<String>| value.unwrap_or_default();
    ResolvedAuthor {
        pen_name: text(sparse.pen_name),
        real_name: text(sparse.real_name),
        phone: text(sparse.phone),
        email: text(sparse.email),
        address: text(sparse.address),
        postcode: text(sparse.postcode),
        bio: text(sparse.bio),
        id_number: text(sparse.id_number),
        bank: text(sparse.bank),
        attach: sparse.attach.unwrap_or(false),
        with_sensitive: sparse.with_sensitive.unwrap_or(false),
    }
}
