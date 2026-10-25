//! 模块认证：**签名清单 + 载荷 SHA-256 锚定**。
//!
//! 认证的真源只有一份：**我们签名的清单**。核心内置公钥验签，**绝不采信模块目录里的
//! 任何"我是认证的"文件**；清单里每一行把「模块 id + 版本 + 载荷哈希 + 第几版 + 日期」
//! 绑在一起，所以"认证过的模块发个新版本继续挂旧证"也过不了。
//!
//! # 诚实边界
//!
//! 认证标识**不是安全凭据**，是一行**可核对的公开声明**：
//! - 它可以被伪造——任何人 fork 一个"全认证版"客户端就能把标识改成全绿（AGPL 拦不住）；
//! - 它防的是"顺手冒充"，防不了"处心积虑"；价值在**可核对**（短指纹 + 公开清单），
//!   不在"不可能假"；
//! - 模块自己的 WebView 里**不许画官方认证徽标**；徽标只能由核心的框渲染。
//!
//! # 清单格式
//!
//! ```json
//! {
//!   "format": 1,
//!   "entries": [
//!     { "module_id": "app.example.map", "version": "1.0.0",
//!       "payload_sha256": "<64 hex>", "edition": 1, "certified_at": 1790000000000 }
//!   ],
//!   "signature": "<128 hex>"
//! }
//! ```
//!
//! 签名覆盖 [`CertificationSet::canonical_message`] 生成的规范文本（按 module_id /
//! version / 哈希排序，字段用 `\t` 分隔）——不依赖 JSON 序列化的空格/顺序。

use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::error::{codes, Error, Result};

pub const CERT_FORMAT: u32 = 1;

/// 官方公钥（32 字节十六进制）。**发布时替换**；空 = 本构建不认证任何模块。
///
/// 私钥永远不进仓库；签发在离线环境完成，签好的清单随版本内置。
const OFFICIAL_PUBLIC_KEY_HEX: &str = "088efbd89a5ae653ccb28f1b1773602ff43006acf4d161bf4fb8171bd9df6300";

/// 官方签名清单（JSON）。与公钥一起替换；空 = 不认证。
const OFFICIAL_CERTIFICATIONS_JSON: &str = r#"{"format":1,"entries":[],"signature":"1b2de4b930d4589790cd81968cf807418fb66ff76c66895ef555fda2ce7b0ce54ad491cfad729f316eeb192b210c7ff2fc2647f85c42f3ec80218f60fe020209"}"#;

/// 清单里的一行：认证绑定到**载荷哈希**，不是只绑名字/版本。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Certification {
    pub module_id: String,
    pub version: String,
    pub payload_sha256: String,
    pub edition: u32,
    pub certified_at: i64,
}

impl Certification {
    pub fn edition(&self) -> u32 {
        self.edition
    }

    /// 给人看的日期（unix 毫秒 → `YYYY-MM-DD`；只用于界面展示）。
    pub fn date_text(&self) -> String {
        let days = self.certified_at.div_euclid(86_400_000);
        let (year, month, day) = civil_from_days(days);
        format!("{year:04}-{month:02}-{day:02}")
    }
}

/// 一份**已验签**的认证清单。
#[derive(Debug, Clone, Default)]
pub struct CertificationSet {
    entries: Vec<Certification>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CertFile {
    format: u32,
    entries: Vec<Certification>,
    signature: String,
}

impl CertificationSet {
    pub fn empty() -> Self {
        Self::default()
    }

    /// 本构建内置的官方清单。**官方公钥还没接入，所以现在为空**；
    /// 发布时换入 `OFFICIAL_PUBLIC_KEY_HEX` 与 `OFFICIAL_CERTIFICATIONS_JSON`。
    pub fn built_in() -> Self {
        if OFFICIAL_PUBLIC_KEY_HEX.is_empty() || OFFICIAL_CERTIFICATIONS_JSON.is_empty() {
            return Self::empty();
        }
        // 内置清单坏了就**失败关闭**（当作没认证），绝不把坏清单当好清单。
        Self::from_signed_json(OFFICIAL_CERTIFICATIONS_JSON, OFFICIAL_PUBLIC_KEY_HEX)
            .unwrap_or_default()
    }

    pub fn entries(&self) -> &[Certification] {
        &self.entries
    }

    /// 解析并验签。`public_key_hex` 是 32 字节公钥的十六进制。
    pub fn from_signed_json(json: &str, public_key_hex: &str) -> Result<Self> {
        let file: CertFile = serde_json::from_str(json)
            .map_err(|error| cert_error(format!("json: {error}")))?;
        if file.format != CERT_FORMAT {
            return Err(cert_error(format!(
                "unsupported format {} (expected {CERT_FORMAT})",
                file.format
            )));
        }
        for entry in &file.entries {
            validate_entry(entry)?;
        }
        let mut seen = std::collections::BTreeSet::new();
        for entry in &file.entries {
            let key = (entry.module_id.clone(), entry.version.clone(), entry.payload_sha256.clone());
            if !seen.insert(key) {
                return Err(cert_error("duplicate certification entry".to_string()));
            }
        }

        let key_bytes = hex_fixed::<32>(public_key_hex, "public key")?;
        let signature_bytes = hex_fixed::<64>(&file.signature, "signature")?;
        let key = VerifyingKey::from_bytes(&key_bytes)
            .map_err(|error| cert_error(format!("public key: {error}")))?;
        let signature = Signature::from_slice(&signature_bytes)
            .map_err(|error| cert_error(format!("signature: {error}")))?;
        let message = Self::canonical_message(&file.entries);
        key.verify_strict(message.as_bytes(), &signature)
            .map_err(|error| cert_error(format!("signature: {error}")))?;

        Ok(Self { entries: file.entries })
    }

    /// 规范签名文本：**排序后**逐行 `id\tversion\thash\tedition\tcertified_at`。
    ///
    /// 公开出来是为了让签发工具与第三方核对器用**同一份**文本，不各写一套。
    pub fn canonical_message(entries: &[Certification]) -> String {
        let mut sorted = entries.to_vec();
        sorted.sort_by(|left, right| {
            (&left.module_id, &left.version, &left.payload_sha256).cmp(&(
                &right.module_id,
                &right.version,
                &right.payload_sha256,
            ))
        });
        let mut message = String::from("yanmo-module-cert-v1\n");
        for entry in sorted {
            message.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\n",
                entry.module_id,
                entry.version,
                entry.payload_sha256,
                entry.edition,
                entry.certified_at
            ));
        }
        message
    }

    /// 按「id + 版本 + 载荷哈希」查认证；少一个都不算。
    pub fn lookup(
        &self,
        module_id: &str,
        version: &str,
        payload_sha256: &str,
    ) -> Option<&Certification> {
        self.entries.iter().find(|entry| {
            entry.module_id == module_id
                && entry.version == version
                && entry.payload_sha256 == payload_sha256
        })
    }
}

fn validate_entry(entry: &Certification) -> Result<()> {
    if !yanmo_proto::is_valid_module_id(&entry.module_id) {
        return Err(cert_error(format!("bad module id: {}", entry.module_id)));
    }
    if entry.version.is_empty() || entry.version.len() > 32 {
        return Err(cert_error("bad version".to_string()));
    }
    if hex_fixed::<32>(&entry.payload_sha256, "payload hash").is_err() {
        return Err(cert_error("bad payload hash".to_string()));
    }
    if entry.edition == 0 {
        return Err(cert_error("edition must be >= 1".to_string()));
    }
    if entry.certified_at <= 0 {
        return Err(cert_error("certified_at must be > 0".to_string()));
    }
    Ok(())
}

fn cert_error(detail: String) -> Error {
    Error::invalid_with(codes::MODULE_CERT_INVALID, [("detail", detail)])
}

fn hex_fixed<const N: usize>(text: &str, what: &str) -> Result<[u8; N]> {
    if text.len() != N * 2 || !text.is_ascii() {
        return Err(cert_error(format!("{what}: expected {} hex chars", N * 2)));
    }
    let mut output = [0u8; N];
    for (index, chunk) in text.as_bytes().chunks(2).enumerate() {
        let high = hex_digit(chunk[0]).ok_or_else(|| cert_error(format!("{what}: not hex")))?;
        let low = hex_digit(chunk[1]).ok_or_else(|| cert_error(format!("{what}: not hex")))?;
        output[index] = (high << 4) | low;
    }
    Ok(output)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// unix 天数 → 公历（Howard Hinnant 的算法；只用于展示）。
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32, d as u32)
}

#[cfg(test)]
mod official_tests {
    use super::*;

    #[test]
    fn official_key_and_empty_signed_list_are_wired() {
        assert!(!OFFICIAL_PUBLIC_KEY_HEX.is_empty());
        let set = CertificationSet::from_signed_json(
            OFFICIAL_CERTIFICATIONS_JSON,
            OFFICIAL_PUBLIC_KEY_HEX,
        )
        .expect("official empty list must verify");
        assert!(set.entries().is_empty());
    }
}
