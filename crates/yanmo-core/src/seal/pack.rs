//! 载荷的编解码：**一包文件 ↔ 一串字节**。
//!
//! 封包解开之后就是它。字段定宽、文件按路径字典序排，所以同一包文件永远编出同一串
//! 字节——验收要靠这条比「两次编出来是不是逐字节一样」。
//!
//! # 路径规矩
//!
//! 只收**相对路径**且用 `/` 分隔：`..`、盘符、前导 `/` 一律当场拒绝。这条不是洁癖——
//! 封包是可能从别处拿回来的，解包时不设防就是「写文件写到盘上任意位置」。

use crate::error::{codes, Error, Result};

/// 包里的一个文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackedFile {
    /// 相对路径（`/` 分隔，不带 `..`）
    pub path: String,
    /// 文件内容
    pub body: Vec<u8>,
}

/// 编码：按路径字典序排好再写（确定性）。
pub fn encode(files: &[PackedFile]) -> Result<Vec<u8>> {
    let mut ordered: Vec<&PackedFile> = files.iter().collect();
    ordered.sort_by(|left, right| left.path.cmp(&right.path));
    if let Some(pair) = ordered.windows(2).find(|pair| pair[0].path == pair[1].path) {
        return Err(Error::invalid_with(
            codes::SEAL_BAD_PATH,
            [("path", pair[0].path.clone())],
        ));
    }
    let mut out = Vec::new();
    out.extend_from_slice(&(ordered.len() as u32).to_le_bytes());
    for file in ordered {
        let path = checked_path(&file.path)?;
        out.extend_from_slice(&(path.len() as u32).to_le_bytes());
        out.extend_from_slice(path.as_bytes());
        out.extend_from_slice(&(file.body.len() as u64).to_le_bytes());
        out.extend_from_slice(&file.body);
    }
    Ok(out)
}

/// 解码：长度逐个校验，多一个字节少一个字节都当场说清。
pub fn decode(bytes: &[u8]) -> Result<Vec<PackedFile>> {
    let mut cursor = Cursor { bytes, at: 0 };
    let count = cursor.take(4)? as u32;
    let mut out = Vec::new();
    for _ in 0..count {
        let path_len = cursor.take(4)? as u32;
        let path = String::from_utf8(cursor.bytes(path_len as usize)?.to_vec())
            .map_err(|_| Error::invalid(codes::SEAL_BAD_PACKAGE))?;
        checked_path(&path)?;
        let body_len = cursor.take(8)? as u64;
        let body = cursor.bytes(body_len as usize)?.to_vec();
        out.push(PackedFile { path, body });
    }
    if cursor.at != bytes.len() {
        return Err(Error::invalid(codes::SEAL_BAD_PACKAGE));
    }
    Ok(out)
}

/// 路径体检：相对、`/` 分隔、不带 `..`。
fn checked_path(path: &str) -> Result<&str> {
    let safe = !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && path.split('/').all(|part| !part.is_empty() && part != "." && part != "..");
    if safe {
        Ok(path)
    } else {
        Err(Error::invalid_with(codes::SEAL_BAD_PATH, [("path", path.to_string())]))
    }
}

/// 读取游标：越界就报「这个包坏了」，不给半截数据。
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    /// 读定宽整数（小端），返回它的数值。
    fn take(&mut self, width: usize) -> Result<u64> {
        let raw = self.bytes(width)?;
        let mut value: u64 = 0;
        for (index, byte) in raw.iter().enumerate() {
            value |= u64::from(*byte) << (8 * index);
        }
        Ok(value)
    }

    /// 读一段字节。
    fn bytes(&mut self, width: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(width)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| Error::invalid(codes::SEAL_BAD_PACKAGE))?;
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, body: &str) -> PackedFile {
        PackedFile { path: path.to_string(), body: body.as_bytes().to_vec() }
    }

    #[test]
    fn 编解码是互逆的_且顺序确定() {
        let files = vec![file("b/2.txt", "乙"), file("a/1.txt", "甲"), file("c.txt", "丙")];
        let first = encode(&files).unwrap();
        let second = encode(&files.iter().rev().cloned().collect::<Vec<_>>()).unwrap();
        assert_eq!(first, second, "喂进去的顺序不该影响编出来的字节");
        let back = decode(&first).unwrap();
        assert_eq!(
            back,
            vec![file("a/1.txt", "甲"), file("b/2.txt", "乙"), file("c.txt", "丙")],
            "按路径字典序回来"
        );
    }

    #[test]
    fn 坏路径一律拒绝() {
        for bad in ["", "/etc/passwd", "../逃出去", "a/../b", "a\\b", "x:y", "a//b"] {
            assert!(encode(&[file(bad, "x")]).is_err(), "不该收下：{bad}");
        }
    }

    #[test]
    fn 截断或多余字节都算坏包() {
        let bytes = encode(&[file("a.txt", "甲"), file("b.txt", "乙")]).unwrap();
        assert!(decode(&bytes[..bytes.len() - 1]).is_err(), "少一个字节");
        let mut extra = bytes.clone();
        extra.push(0);
        assert!(decode(&extra).is_err(), "多一个字节");
    }

    #[test]
    fn 同名文件当场拒绝() {
        assert!(encode(&[file("a.txt", "甲"), file("a.txt", "乙")]).is_err());
    }
}
