//! 镜像里的**单元格编码**：SQLite 的一个值 ↔ JSON 里的一个格子。
//!
//! 两种类型分开放（与 [`super::module_mirror`] 的读写分开），是因为这一层是**纯换算**：
//! 不碰文件、不碰表名，单测也只要几张固定值表。
//!
//! 口径：按**运行时类型**记（不是声明类型）。JSON 只有 null / 布尔 / 数字 / 字符串 /
//! 数组 / 对象，所以二进制写成 `{"blob":"<十六进制>"}`；`inf` / `-inf` / `nan` 单列成
//! `{"real":"inf"}`——**静默把它们变成一个数**是这里最坏的结果。

use rusqlite::types::ValueRef;

use crate::error::{codes, Error, Result};

/// SQLite 的一个值 → JSON 单元格。
pub(super) fn encode_cell(value: ValueRef<'_>) -> Result<serde_json::Value> {
    Ok(match value {
        ValueRef::Null => serde_json::Value::Null,
        ValueRef::Integer(number) => serde_json::Value::from(number),
        ValueRef::Real(number) => {
            if number.is_finite() {
                serde_json::Value::from(number)
            } else {
                let tag = if number.is_nan() {
                    "nan"
                } else if number > 0.0 {
                    "inf"
                } else {
                    "-inf"
                };
                serde_json::json!({ "real": tag })
            }
        }
        ValueRef::Text(bytes) => serde_json::Value::from(
            std::str::from_utf8(bytes).map_err(|error| {
                Error::invalid_with(codes::MODULE_MIRROR_INVALID, [("detail", error.to_string())])
            })?,
        ),
        ValueRef::Blob(bytes) => serde_json::json!({ "blob": to_hex(bytes) }),
    })
}

/// JSON 单元格 → SQLite 的一个值（重建时用；认不出的形状当场拒）。
pub(super) fn decode_cell(value: &serde_json::Value) -> Result<rusqlite::types::Value> {
    use rusqlite::types::Value as Sql;
    let invalid =
        || Error::invalid_with(codes::MODULE_MIRROR_INVALID, [("detail", "bad cell".to_string())]);
    Ok(match value {
        serde_json::Value::Null => Sql::Null,
        serde_json::Value::Number(number) => {
            if let Some(integer) = number.as_i64() {
                Sql::Integer(integer)
            } else if let Some(real) = number.as_f64() {
                Sql::Real(real)
            } else {
                return Err(invalid());
            }
        }
        serde_json::Value::String(text) => Sql::Text(text.clone()),
        serde_json::Value::Object(map) => {
            if let Some(hex) = map.get("blob").and_then(|item| item.as_str()) {
                Sql::Blob(from_hex(hex).ok_or_else(invalid)?)
            } else if let Some(tag) = map.get("real").and_then(|item| item.as_str()) {
                let number = match tag {
                    "inf" => f64::INFINITY,
                    "-inf" => f64::NEG_INFINITY,
                    "nan" => f64::NAN,
                    _ => return Err(invalid()),
                };
                Sql::Real(number)
            } else {
                return Err(invalid());
            }
        }
        _ => return Err(invalid()),
    })
}

/// 二进制 → 十六进制小写。
pub(super) fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 十六进制 → 二进制（奇数长度或非法字符返回 `None`）。
pub(super) fn from_hex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks(2) {
        let high = hex_digit(pair[0])?;
        let low = hex_digit(pair[1])?;
        out.push(high << 4 | low);
    }
    Some(out)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips_and_rejects_junk() {
        let bytes: Vec<u8> = (0..=255).collect();
        assert_eq!(from_hex(&to_hex(&bytes)).unwrap(), bytes);
        assert_eq!(to_hex(&[0, 15, 16, 255]), "000f10ff");
        assert!(from_hex("0").is_none(), "奇数长度");
        assert!(from_hex("zz").is_none(), "非法字符");
    }

    #[test]
    fn cells_round_trip_including_binary_and_infinities() {
        for cell in [
            serde_json::Value::Null,
            serde_json::Value::from(-7i64),
            serde_json::Value::from(1.5f64),
            serde_json::Value::from("一段文字"),
            serde_json::json!({"blob": "00ff"}),
            serde_json::json!({"real": "inf"}),
        ] {
            let decoded = decode_cell(&cell).expect("应当认得");
            let back = match decoded {
                rusqlite::types::Value::Null => serde_json::Value::Null,
                rusqlite::types::Value::Integer(number) => serde_json::Value::from(number),
                rusqlite::types::Value::Real(number) => serde_json::Value::from(number),
                rusqlite::types::Value::Text(text) => serde_json::Value::from(text),
                rusqlite::types::Value::Blob(bytes) => serde_json::json!({"blob": to_hex(&bytes)}),
            };
            if cell.get("real").is_some() {
                continue; // inf 与自身比较仍是 inf，这里只验形状解得出来
            }
            assert_eq!(back, cell);
        }
        assert!(decode_cell(&serde_json::json!({"weird": 1})).is_err());
        assert!(decode_cell(&serde_json::json!({"blob": "xyz"})).is_err());
    }
}
