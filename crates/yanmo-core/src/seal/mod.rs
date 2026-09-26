//! 备份包封印：把一包文件**整包加密**成一个文件，拿给网盘或别人也看不出里面是什么。
//!
//! # 形态
//!
//! 封好的包是一个文件（扩展名 `SEAL_EXT`）：头部是**最小明文**（算法、参数、盐、
//! 两个钥匙槽），其余全是密文。**诚实边界**：文件名与「什么时候写的」依旧在文件系统上
//! 看得见——藏得住内容，藏不住「你有一份备份」这件事。
//!
//! # 口令怎么变成钥匙
//!
//! 口令与恢复码各自经 **Argon2id** 派生出一把钥匙，各自包住**同一把随机数据钥匙**
//! （换口令不必重加密整个包）；数据本身用 **XChaCha20-Poly1305** 加密。
//! **整个包头作为附加认证数据**：改一个字节的头也会当场解不开。
//!
//! # 两条取舍（写在这里，不写在广告里）
//!
//! 1. **加密与「服务端全文检索」根本矛盾**：加密包里连文件名都看不到，网盘端当然没法
//!    帮你搜正文。要能搜就只能在自己机器上解密后搜。
//! 2. **钥匙丢了就是数据丢了**：没有后门。恢复码是第二把钥匙，不是「找回口令」的后门。

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use zeroize::Zeroizing;

use crate::error::{codes, Error, Result};

pub mod keys;
pub mod pack;

pub use keys::{generate_recovery_code, normalize_recovery, group_recovery, Secret, RECOVERY_CHARS};
pub use pack::PackedFile;

/// 封包的扩展名（不含点）。
pub const SEAL_EXT: &str = "ympkg";

/// 文件头的魔数：一眼认出「这是一个封包」，也不至于把别的文件当封包开。
const MAGIC: &[u8] = b"YANMOSEAL";
const FORMAT_VERSION: u8 = 1;
const KDF_ARGON2ID: u8 = 1;
const SALT_LEN: usize = 32;
const NONCE_LEN: usize = 24;
const KEY_LEN: usize = 32;
const TAG_LEN: usize = 16;
const WRAP_LEN: usize = KEY_LEN + TAG_LEN;
const SLOT_LEN: usize = 1 + NONCE_LEN + WRAP_LEN;
const SLOT_COUNT: usize = 2;
/// 文件头总长（定死的，好让「整头当附加认证数据」这件事有确定的边界）。
pub const HEADER_LEN: usize = MAGIC.len() + 1 + 1 + 12 + SALT_LEN + NONCE_LEN + 1 + SLOT_COUNT * SLOT_LEN;

/// 密钥派生的代价参数。**写进包头**，所以将来调价不影响老包。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    /// 内存（KiB）
    pub memory_kib: u32,
    /// 迭代轮数
    pub rounds: u32,
    /// 并行度
    pub lanes: u32,
}

impl KdfParams {
    /// 默认档：64 MiB / 4 轮 / 1 路——离线爆破要花真金白银的算力。
    pub const DEFAULT: Self = Self { memory_kib: 64 * 1024, rounds: 4, lanes: 1 };
    /// 验收专用的轻档：**只给测试用**，它挡不住爆破。
    pub const FAST: Self = Self { memory_kib: 64, rounds: 1, lanes: 1 };

    fn to_argon2(self) -> Result<Argon2<'static>> {
        let params = Params::new(self.memory_kib, self.rounds, self.lanes, Some(KEY_LEN))
            .map_err(|_| Error::invalid(codes::SEAL_OPEN_FAILED))?;
        Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
    }
}

/// 封包：两把钥匙（口令 + 恢复码）都开得了的密文。
pub fn seal(files: &[PackedFile], passphrase: &str, recovery: &str) -> Result<Vec<u8>> {
    seal_with(files, passphrase, recovery, KdfParams::DEFAULT)
}

/// 封包（指定代价参数；轻档只给测试用）。
pub fn seal_with(
    files: &[PackedFile],
    passphrase: &str,
    recovery: &str,
    params: KdfParams,
) -> Result<Vec<u8>> {
    let payload = pack::encode(files)?;
    let secrets = [
        Secret::Passphrase(passphrase.to_string()),
        Secret::Recovery(recovery.to_string()),
    ];
    // 头部先拼到定长，再当附加认证数据加密正文——所以头里任何一处被改都开不了。
    let mut header = Vec::with_capacity(HEADER_LEN);
    header.extend_from_slice(MAGIC);
    header.push(FORMAT_VERSION);
    header.push(KDF_ARGON2ID);
    for value in [params.memory_kib, params.rounds, params.lanes] {
        header.extend_from_slice(&value.to_le_bytes());
    }
    let mut salt = [0u8; SALT_LEN];
    fill_random(&mut salt)?;
    header.extend_from_slice(&salt);
    let mut nonce = [0u8; NONCE_LEN];
    fill_random(&mut nonce)?;
    header.extend_from_slice(&nonce);
    header.push(SLOT_COUNT as u8);

    let mut data_key = Zeroizing::new([0u8; KEY_LEN]);
    fill_random(&mut *data_key)?;
    for secret in &secrets {
        header.push(secret.slot());
        let mut wrap_nonce = [0u8; NONCE_LEN];
        fill_random(&mut wrap_nonce)?;
        header.extend_from_slice(&wrap_nonce);
        let kek = derive(secret, &salt, params)?;
        let wrapped = cipher(&kek[..])?.encrypt(
            &nonce_of(&wrap_nonce)?,
            Payload { msg: &data_key[..], aad: &[] },
        );
        header.extend_from_slice(&wrapped.map_err(|_| Error::invalid(codes::SEAL_OPEN_FAILED))?);
    }
    // i18n-allow-next-line: 开发者断言（崩在测试/调试构建里），不进界面
    debug_assert_eq!(header.len(), HEADER_LEN, "文件头定长，别悄悄改长了");

    let body = cipher(&data_key[..])?
        .encrypt(&nonce_of(&nonce)?, Payload { msg: &payload, aad: &header })
        .map_err(|_| Error::invalid(codes::SEAL_OPEN_FAILED))?;
    header.extend_from_slice(&body);
    Ok(header)
}

/// 开包：拿**任意一把**钥匙（口令或恢复码）。
///
/// 打不开只回一句「打不开」：**钥匙不对与包被改动是同一句话**——多说一句
/// 「你钥匙错了」既可能撒谎，也可能替攻击者缩小猜的范围。
pub fn unseal(blob: &[u8], secret: &Secret) -> Result<Vec<PackedFile>> {
    if blob.len() < HEADER_LEN || &blob[..MAGIC.len()] != MAGIC {
        return Err(Error::invalid(codes::SEAL_OPEN_FAILED));
    }
    let header = &blob[..HEADER_LEN];
    let version = header[MAGIC.len()];
    if version != FORMAT_VERSION {
        return Err(Error::invalid_with(
            codes::SEAL_UNSUPPORTED_VERSION,
            [("version", version.to_string())],
        ));
    }
    if header[MAGIC.len() + 1] != KDF_ARGON2ID {
        return Err(Error::invalid(codes::SEAL_OPEN_FAILED));
    }
    let at = MAGIC.len() + 2;
    let params = KdfParams {
        memory_kib: read_u32(&header[at..]),
        rounds: read_u32(&header[at + 4..]),
        lanes: read_u32(&header[at + 8..]),
    };
    let salt = &header[at + 12..at + 12 + SALT_LEN];
    let nonce = &header[at + 12 + SALT_LEN..at + 12 + SALT_LEN + NONCE_LEN];
    let slots_at = at + 12 + SALT_LEN + NONCE_LEN + 1;
    let slot = (0..SLOT_COUNT)
        .map(|index| &header[slots_at + index * SLOT_LEN..slots_at + (index + 1) * SLOT_LEN])
        .find(|slot| slot[0] == secret.slot())
        .ok_or_else(|| Error::invalid(codes::SEAL_OPEN_FAILED))?;

    let kek = derive(secret, salt, params)?;
    let data_key = cipher(&kek[..])?
        .decrypt(
            &nonce_of(&slot[1..1 + NONCE_LEN])?,
            Payload { msg: &slot[1 + NONCE_LEN..], aad: &[] },
        )
        .map_err(|_| Error::invalid(codes::SEAL_OPEN_FAILED))?;
    let payload = cipher(&data_key[..])?
        .decrypt(
            &nonce_of(nonce)?,
            Payload { msg: &blob[HEADER_LEN..], aad: header },
        )
        .map_err(|_| Error::invalid(codes::SEAL_OPEN_FAILED))?;
    pack::decode(&payload)
}

/// 从钥匙材料派生一把 32 字节的钥匙（盐里带上槽号：两把钥匙各走各的派生）。
fn derive(secret: &Secret, salt: &[u8], params: KdfParams) -> Result<Zeroizing<[u8; KEY_LEN]>> {
    let material = secret.material()?;
    let mut salt_with_slot = Vec::with_capacity(salt.len() + 1);
    salt_with_slot.push(secret.slot());
    salt_with_slot.extend_from_slice(salt);
    let mut out = Zeroizing::new([0u8; KEY_LEN]);
    params
        .to_argon2()?
        .hash_password_into(&material, &salt_with_slot, &mut *out)
        .map_err(|_| Error::invalid(codes::SEAL_OPEN_FAILED))?;
    Ok(out)
}

fn cipher(key: &[u8]) -> Result<XChaCha20Poly1305> {
    let key = Key::try_from(key).map_err(|_| Error::invalid(codes::SEAL_OPEN_FAILED))?;
    Ok(XChaCha20Poly1305::new(&key))
}

/// 24 字节的随机数（长度对不上就当坏包，不给半截）。
fn nonce_of(bytes: &[u8]) -> Result<XNonce> {
    XNonce::try_from(bytes).map_err(|_| Error::invalid(codes::SEAL_OPEN_FAILED))
}

fn fill_random(target: &mut [u8]) -> Result<()> {
    getrandom::fill(target).map_err(|_| Error::invalid(codes::SEAL_RANDOM_FAILED))
}

fn read_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> Vec<PackedFile> {
        vec![
            PackedFile { path: "manifest.json".into(), body: b"{\"kind\":\"yanmo-backup\"}".to_vec() },
            PackedFile { path: "成稿/第一章.txt".into(), body: "那一年的雪下得很大。".as_bytes().to_vec() },
        ]
    }

    fn seal_fast(files: &[PackedFile]) -> Vec<u8> {
        seal_with(files, "口令：不要告诉别人", "1M2H-J4K6-N8PQ-RSTV-WXY0-2345", KdfParams::FAST).unwrap()
    }

    #[test]
    fn 两把钥匙都开得了_且还原逐字节一样() {
        let blob = seal_fast(&files());
        for secret in [
            Secret::Passphrase("口令：不要告诉别人".to_string()),
            Secret::Recovery("1M2H-J4K6-N8PQ-RSTV-WXY0-2345".to_string()),
        ] {
            assert_eq!(unseal(&blob, &secret).unwrap(), files());
        }
    }

    #[test]
    fn 错钥匙一律只说打不开() {
        let blob = seal_fast(&files());
        assert!(unseal(&blob, &Secret::Passphrase("猜的".to_string())).is_err());
        assert!(unseal(&blob, &Secret::Recovery("0000-0000-0000-0000-0000-0000".to_string())).is_err());
    }

    #[test]
    fn 头或密文动一个字节就打不开() {
        let blob = seal_fast(&files());
        // 79 = 钥匙槽数量那一格：它不参与钥匙派生，专靠「整头当附加认证数据」兜住
        for at in [10usize, 40, 60, 79, HEADER_LEN - 1, HEADER_LEN, blob.len() - 1] {
            let mut broken = blob.clone();
            broken[at] ^= 0x5a;
            assert!(unseal(&broken, &Secret::Passphrase("口令：不要告诉别人".to_string())).is_err(), "第 {at} 字节");
        }
    }

    #[test]
    fn 同一口令两包也长得完全不一样() {
        let first = seal_fast(&files());
        let second = seal_fast(&files());
        assert_ne!(first, second, "盐、随机数与数据钥匙都是随机的");
    }

    #[test]
    fn 认不得的版本如实说_不猜() {
        let mut blob = seal_fast(&files());
        blob[MAGIC.len()] = FORMAT_VERSION + 9;
        let error = unseal(&blob, &Secret::Passphrase("口令：不要告诉别人".to_string())).unwrap_err();
        assert!(matches!(error, Error::Invalid { code, .. } if code == codes::SEAL_UNSUPPORTED_VERSION));
    }

    #[test]
    fn 恢复码随手抄成小写带空格照样开得了() {
        let code = generate_recovery_code().unwrap();
        let blob = seal_with(&files(), "口令", &code, KdfParams::FAST).unwrap();
        let messy = Secret::Recovery(code.to_lowercase().replace('-', " "));
        assert_eq!(unseal(&blob, &messy).unwrap(), files());
    }

    #[test]
    fn 空口令当场拒绝_不给封出一个没有门的包() {
        let error = seal_with(&files(), "", "1M2H-J4K6-N8PQ-RSTV-WXY0-2345", KdfParams::FAST).unwrap_err();
        assert!(matches!(error, Error::Invalid { code, .. } if code == codes::SEAL_EMPTY_SECRET));
    }
}
