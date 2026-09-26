//! 备份包的**封与开**：把写好的备份包目录封成一个文件，或者把封包解回目录。
//!
//! 与 [`super::backup`]（写包、清单、账本）分开：那边管「备份这件事」，这边只管
//! 「包 ↔ 密文文件」的转换。封包的密码学在 [`crate::seal`]，这里只做磁盘上的搬运。
//!
//! # 两条纪律
//!
//! 1. **先解密、再写盘**：解不开就一个文件都不落——不留半截解密产物在盘上；
//! 2. **只写包里写的路径**：路径体检在 [`crate::seal::pack`] 里做过（不许 `..` / 盘符 /
//!    前导 `/`），这里照单落盘，不替包里没写的东西补目录。

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{codes, Error, Result};
use crate::seal::{self, PackedFile, Secret};

/// 把一个备份包目录封成一个密文文件（`研墨备份-xxx` 目录 → `研墨备份-xxx.ympkg`）。
///
/// 目录按相对路径字典序读（编码侧还会再排一次）；空文件也照收。
pub fn seal_dir(dir: &Path, out: &Path, passphrase: &str, recovery: &str) -> Result<()> {
    let files = read_dir(dir, dir)?;
    let blob = seal::seal(&files, passphrase, recovery)?;
    crate::atomic::write_atomic(out, &blob)?;
    Ok(())
}

/// 把封包解回一个目录（**解不开就一个文件都不写**）。
pub fn open_sealed(file: &Path, out_dir: &Path, secret: &Secret) -> Result<()> {
    let blob = fs::read(file).map_err(|_| Error::invalid_with(codes::BACKUP_RESTORE_SOURCE_INVALID, [
        ("path", file.display().to_string()),
    ]))?;
    let files = seal::unseal(&blob, secret)?;
    for item in &files {
        let target = out_dir.join(PathBuf::from(&item.path));
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|_| Error::invalid_with(codes::SEAL_BAD_PATH, [
                ("path", item.path.clone()),
            ]))?;
        }
        fs::write(&target, &item.body).map_err(|_| Error::invalid_with(codes::SEAL_BAD_PATH, [
            ("path", item.path.clone()),
        ]))?;
    }
    Ok(())
}

/// 递归读一个目录成「相对路径 → 内容」（相对路径一律用 `/` 分隔）。
fn read_dir(root: &Path, dir: &Path) -> Result<Vec<PackedFile>> {
    let mut out = Vec::new();
    let entries = fs::read_dir(dir).map_err(|_| Error::invalid_with(codes::SEAL_BAD_PACKAGE, []))?;
    for entry in entries {
        let entry = entry.map_err(|_| Error::invalid(codes::SEAL_BAD_PACKAGE))?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_| Error::invalid(codes::SEAL_BAD_PACKAGE))?
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        if path.is_dir() {
            out.extend(read_dir(root, &path)?);
        } else {
            let body = fs::read(&path).map_err(|_| Error::invalid_with(codes::SEAL_BAD_PATH, [
                ("path", relative.clone()),
            ]))?;
            out.push(PackedFile { path: relative, body });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_fixture(dir: &Path) {
        fs::create_dir_all(dir.join("成稿")).unwrap();
        fs::write(dir.join("manifest.json"), b"{\"kind\":\"yanmo-backup\"}").unwrap();
        fs::write(dir.join("成稿").join("第一章.txt"), "那一年的雪下得很大。").unwrap();
    }

    #[test]
    fn 封了再开_目录里逐字节一样() {
        let source = tempdir().unwrap();
        let target = tempdir().unwrap();
        write_fixture(source.path());
        let sealed = target.path().join("研墨备份-今天.ympkg");
        let opened = target.path().join("解开");

        seal_dir(source.path(), &sealed, "口令", "1M2H-J4K6-N8PQ-RSTV-WXY0-2345").unwrap();
        assert!(sealed.exists(), "封包落了盘");
        open_sealed(&sealed, &opened, &Secret::Passphrase("口令".to_string())).unwrap();
        assert_eq!(
            fs::read(opened.join("成稿").join("第一章.txt")).unwrap(),
            fs::read(source.path().join("成稿").join("第一章.txt")).unwrap()
        );
        assert_eq!(
            fs::read(opened.join("manifest.json")).unwrap(),
            fs::read(source.path().join("manifest.json")).unwrap()
        );
    }

    #[test]
    fn 解不开就一个文件都不写() {
        let source = tempdir().unwrap();
        let target = tempdir().unwrap();
        write_fixture(source.path());
        let sealed = target.path().join("研墨备份-今天.ympkg");
        seal_dir(source.path(), &sealed, "口令", "1M2H-J4K6-N8PQ-RSTV-WXY0-2345").unwrap();
        let opened = target.path().join("解开");

        let error = open_sealed(&sealed, &opened, &Secret::Passphrase("猜的".to_string())).unwrap_err();
        assert!(matches!(error, Error::Invalid { code, .. } if code == codes::SEAL_OPEN_FAILED));
        assert!(!opened.exists(), "解不开就别留半截产物");
    }
}
