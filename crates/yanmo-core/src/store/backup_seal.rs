//! 备份包的**封与开**：把写好的备份包目录封成一个文件，或者把封包解回目录。
//!
//! 与 [`super::backup`]（写包、清单、账本）分开：那边管「备份这件事」，这边只管
//! 「包 ↔ 密文文件」的转换，外加**加密那一份钥匙**（口令只在内存里、恢复码存在本机偏好里，
//! 见 [`super::backup_key`]）。封包的密码学在 [`crate::seal`]，这里只做磁盘上的搬运与体检。
//!
//! # 三条纪律
//!
//! 1. **先解密、再写盘**：解不开就一个文件都不落——不留半截解密产物在盘上；
//! 2. **只写包里写的路径**：路径体检在 [`crate::seal::pack`] 里做过（不许 `..` / 盘符 /
//!    前导 `/`），这里照单落盘，不替包里没写的东西补目录；
//! 3. **封完要当场解开体检一遍**：递给作者的是那个密文文件，只有「它真能解开、里面的东西
//!    真完好」才配叫一份备份——写进去而打不开，比没写更坏（见 [`seal_package`]）。

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{codes, Error, Result};
use crate::seal::{self, PackedFile, Secret};

use super::backup::{
    entry, outcome, LedgerEntry, SealKey, TargetOutcome, PACKAGE_PREFIX,
};

/// 把一个备份包目录封成一个密文文件（`研墨备份-xxx` 目录 → `研墨备份-xxx.ympkg`）。
///
/// 目录按相对路径字典序读（编码侧还会再排一次）；空文件也照收。
pub fn seal_dir(dir: &Path, out: &Path, passphrase: &str, recovery: &str) -> Result<()> {
    let files = read_dir(dir, dir)?;
    let blob = seal::seal(&files, passphrase, recovery)?;
    crate::atomic::write_atomic(out, &blob)?;
    Ok(())
}

/// 一个包目录对应的密文文件名：`研墨备份-xxx` → `研墨备份-xxx.ympkg`。
pub fn sealed_path(package: &Path) -> PathBuf {
    let name = package.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    package.with_file_name(format!("{name}.{}", crate::seal::SEAL_EXT))
}

/// 这是不是一个加密备份包（`研墨备份-….ympkg` 那个文件）。
///
/// 认它只看两件事：**是我们的名字**（前缀）**且是我们的扩展名**——
/// 不看内容，因为内容正是读不到的那部分。
pub fn is_sealed_package(path: &Path) -> bool {
    let Some(name) = path.file_name().map(|n| n.to_string_lossy().to_string()) else {
        return false;
    };
    path.is_file()
        && name.starts_with(PACKAGE_PREFIX)
        && name.ends_with(&format!(".{}", crate::seal::SEAL_EXT))
}

/// **把这份备份封好**（一次），并当场解开体检一遍。
///
/// 为什么只封一次：所有目标拿到的应当是**同一份密文**——封两遍除了多花两遍 Argon2id
/// （64 MiB / 4 轮），还会让"两个目标上的包逐字节不同"这种糊涂账出现。
///
/// 为什么要解开体检：递给作者的是这个密文文件，**写进去而打不开比没写更坏**
/// （作者会以为自己有备份）。所以：封 → 解开到一个临时目录 → 交给调用方体检
/// （清单/快照/逐书对账那一套）→ 通不过就把密文删掉并如实报原因。
///
/// 返回封好的密文文件路径；失败时返回**给作者看的一句话理由**（同 [`super::backup`] 的风格）。
pub(super) fn seal_package(
    staging: &Path,
    key: &SealKey,
    verify: impl Fn(&Path) -> bool,
) -> std::result::Result<PathBuf, String> {
    let sealed = sealed_path(staging);
    // 与明文包那条路**同一句话**（作者看的是同一栏「上回为什么没成」）
    seal_dir(staging, &sealed, &key.passphrase, &key.recovery)
        .map_err(super::backup::write_failed_reason)?;

    // 解开体检：用**口令**那一把解（两把钥匙都能解，这里只要证明密文本身是好的）
    let probe = staging.with_file_name(format!(
        "{}.verify",
        staging.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
    ));
    let _ = fs::remove_dir_all(&probe);
    let opened = open_sealed(&sealed, &probe, &Secret::Passphrase(key.passphrase.clone()));
    let sound = match opened {
        Ok(()) => verify(&probe),
        Err(error) => {
            let _ = fs::remove_dir_all(&probe);
            let _ = fs::remove_file(&sealed);
            // i18n-allow-next-line: 解包失败的作者可见理由（与 backup.rs 那一族同性质，
            // 收「核心诊断串改码+参数」那条任务时一起改成码 + 参数）
            return Err(format!("封好的包解不开（{error}）——已把这份作废，别拿它当备份"));
        }
    };
    let _ = fs::remove_dir_all(&probe);
    if !sound {
        let _ = fs::remove_file(&sealed);
        // i18n-allow-next-line: 同上（解开之后体检没过，这份作废）
        return Err("封好的包解开之后体检没通过——已把这份作废，别拿它当备份".to_string());
    }
    Ok(sealed)
}

/// 把一个已经封好的包搬进某个备份目标（同名就换序号，**绝不覆盖已有备份**），
/// 并按既有口径记一笔账。
///
/// 与明文包那条路的分别只在于「搬什么」：那边搬一棵目录树，这边搬**一个文件**；
/// 体检与滚动清理的口径完全一样（体检在 [`seal_package`] 里已经做过一遍了）。
#[allow(clippy::too_many_arguments)]
pub(super) fn write_sealed_to_target(
    target: &super::backup::BackupTarget,
    sealed: &Path,
    date: &str,
    stamp: &str,
    keep: usize,
    at: i64,
    fingerprint: &str,
) -> (TargetOutcome, LedgerEntry) {
    let root = PathBuf::from(&target.path);
    if let Err(error) = fs::create_dir_all(&root) {
        let reason = super::backup::write_dir_reason(error);
        return (
            outcome(target, "failed", &reason, "", 0, 0, 0, "", at),
            entry(target, date, stamp, "failed", &reason, "", at),
        );
    }
    // 落地名按**包名**来（`研墨备份-<时间戳>.ympkg`），不抄暂存区那个内部名字：
    // 备份盘上摆着 `.backup-staging-…` 这种东西，作者认不出、列表与滚动清理也认不出。
    let wanted = root.join(format!("{PACKAGE_PREFIX}{stamp}.{}", crate::seal::SEAL_EXT));
    let destination = unique_file(wanted);
    let bytes = match fs::copy(sealed, &destination) {
        Ok(bytes) => bytes,
        Err(error) => {
            let reason = super::backup::write_failed_reason(error);
            let _ = fs::remove_file(&destination);
            return (
                outcome(target, "failed", &reason, "", 0, 0, 0, "", at),
                entry(target, date, stamp, "failed", &reason, "", at),
            );
        }
    };
    // 落地之后再核一眼大小：磁盘满 / 半截文件这一档，光看 `copy` 的成功返回值看不出来
    let landed = fs::metadata(&destination).map(|m| m.len()).unwrap_or(0);
    if landed != bytes {
        let _ = fs::remove_file(&destination);
        // 与明文包那条路**同一句话**，细节走英文技术说明（这一栏本来就常是英文细节）
        let reason =
            super::backup::write_failed_reason(format_args!("copy landed {landed} of {bytes} bytes"));
        return (
            outcome(target, "failed", &reason, "", 0, 0, 0, "", at),
            entry(target, date, stamp, "failed", &reason, "", at),
        );
    }
    let removed = super::backup::prune_packages(&root, keep);
    (
        outcome(target, "written", "", &destination.to_string_lossy(), bytes, 0, removed, fingerprint, at),
        entry(target, date, stamp, "written", "", fingerprint, at),
    )
}

/// 目标目录里同名文件已存在时换一个带序号的（`…-2.ympkg`），100 个都占着就用原名兜底。
fn unique_file(base: PathBuf) -> PathBuf {
    if !base.exists() {
        return base;
    }
    let stem = base
        .file_stem()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let extension = base
        .extension()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    for index in 2..100 {
        let candidate = base.with_file_name(format!("{stem}-{index}.{extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    base
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
