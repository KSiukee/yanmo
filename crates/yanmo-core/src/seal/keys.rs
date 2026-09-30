//! 封包的两把钥匙：**口令**与**恢复码**。
//!
//! 同一份备份包两把都开得了：包头里给每把钥匙各留一个「槽」，槽里装的是同一把随机
//! 数据钥匙（外面再用这把钥匙自己的派生结果包一层）。于是换口令不必重加密整个包，
//! 而作者忘了口令也还有恢复码这条路。
//!
//! # 诚实边界
//!
//! **钥匙丢了就是数据丢了**：这里没有后门，也没有「找回口令」。所以恢复码只在生成
//! 那一次给出来，必须让作者自己抄下来保管——这也是「加密备份包」与「加密 + 服务端能
//! 全文检索」不能兼得之外的第二个取舍。

use zeroize::Zeroizing;

use crate::error::{codes, Error, Result};

/// 恢复码的字符表（Crockford Base32：去掉了 `I` / `L` / `O` / `U`，手抄不易看错）。
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// 恢复码的字符数（**不含**分隔符）：24 × 5 位 = 120 位熵。
pub const RECOVERY_CHARS: usize = 24;

/// 恢复码背后的随机字节数（120 位）。
pub const RECOVERY_BYTES: usize = RECOVERY_CHARS * 5 / 8;

/// 口令槽的编号。
pub const SLOT_PASSPHRASE: u8 = 1;
/// 恢复码槽的编号。
pub const SLOT_RECOVERY: u8 = 2;

/// 一把钥匙。
///
/// 只在内存里活着：**口令不落盘**（应用关掉就没了，下次要重输）；恢复码是长期的那把——
/// 壳把它记在本机偏好里（见 `store::backup_key` 文件头那段说明），同时**请作者抄一份到别处**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Secret {
    /// 口令（作者自己记的那句话）
    Passphrase(String),
    /// 恢复码（`XXXX-XXXX-XXXX-XXXX-XXXX-XXXX`，生成时给一次）
    Recovery(String),
}

impl Secret {
    /// 它在包头里占的是哪个钥匙槽。
    pub fn slot(&self) -> u8 {
        match self {
            Secret::Passphrase(_) => SLOT_PASSPHRASE,
            Secret::Recovery(_) => SLOT_RECOVERY,
        }
    }

    /// 归一化之后的「钥匙材料」，交给密钥派生用。
    ///
    /// - **口令按 UTF-8 原样**：前后空格也是口令的一部分，不替作者猜；
    /// - **恢复码**去掉分隔符、转大写，并把易混字符认成同一个（`O`→`0`、`I`/`L`→`1`）——
    ///   作者抄在纸上一个月以后再敲回来，不该因为看错一个字母就开不了。
    pub fn material(&self) -> Result<Zeroizing<Vec<u8>>> {
        match self {
            Secret::Passphrase(text) => {
                if text.is_empty() {
                    return Err(Error::invalid_with(codes::SEAL_EMPTY_SECRET, []));
                }
                Ok(Zeroizing::new(text.as_bytes().to_vec()))
            }
            Secret::Recovery(text) => Ok(Zeroizing::new(normalize_recovery(text)?.to_vec())),
        }
    }
}

/// 生成一把恢复码（120 位熵）。
///
/// **开启加密**那一刻调用一次；返回的字符串要原样交给作者抄下来，同时由壳记进本机偏好
/// （不是写进备份包本身——包里只带它的派生结果；正因如此，**包里取不回这串码**）。
pub fn generate_recovery_code() -> Result<String> {
    let mut raw = [0u8; RECOVERY_BYTES];
    getrandom::fill(&mut raw).map_err(|_| Error::invalid_with(codes::SEAL_RANDOM_FAILED, []))?;
    Ok(recovery_code_from(&raw))
}

/// 从固定随机字节排一把恢复码（纯逻辑，方便拿死值做验收）。
pub fn recovery_code_from(raw: &[u8; RECOVERY_BYTES]) -> String {
    let mut value: u128 = 0;
    for byte in raw {
        value = (value << 8) | u128::from(*byte);
    }
    let mut chars = Vec::with_capacity(RECOVERY_CHARS);
    for _ in 0..RECOVERY_CHARS {
        chars.push(ALPHABET[(value & 0x1f) as usize]);
        value >>= 5;
    }
    chars.reverse();
    // i18n-allow-next-line: 开发者断言（崩在测试/调试构建里），不进界面
    let text = String::from_utf8(chars).expect("字符表是 ASCII");
    group_recovery(&text)
}

/// 把 24 个字符排成 `XXXX-XXXX-XXXX-XXXX-XXXX-XXXX`（抄写与核对都好认）。
pub fn group_recovery(text: &str) -> String {
    text.as_bytes()
        .chunks(4)
    // i18n-allow-next-line: 开发者断言（崩在测试/调试构建里），不进界面
        .map(|chunk| std::str::from_utf8(chunk).expect("字符表是 ASCII"))
        .collect::<Vec<_>>()
        .join("-")
}

/// 把作者敲进来的恢复码归一化成 15 个字节的钥匙材料。
///
/// 认得下的写法：大小写混着、带不带 `-` / 空格都行、`O` 当 `0`、`I` 与 `L` 当 `1`。
/// 其余（长度不对、有表外的字符）当场说清是哪一类问题，不猜。
pub fn normalize_recovery(text: &str) -> Result<[u8; RECOVERY_BYTES]> {
    let mut chars = Vec::with_capacity(RECOVERY_CHARS);
    for raw in text.chars() {
        let ch = match raw {
            '-' | ' ' | '_' => continue,
            other => other.to_ascii_uppercase(),
        };
        let ch = match ch {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        };
        if !ALPHABET.contains(&(ch as u8)) {
            return Err(Error::invalid(codes::SEAL_BAD_RECOVERY_CODE));
        }
        chars.push(ch as u8);
        if chars.len() > RECOVERY_CHARS {
            return Err(Error::invalid(codes::SEAL_BAD_RECOVERY_CODE));
        }
    }
    if chars.len() != RECOVERY_CHARS {
        return Err(Error::invalid(codes::SEAL_BAD_RECOVERY_CODE));
    }
    let mut value: u128 = 0;
    for ch in &chars {
    // i18n-allow-next-line: 开发者断言（崩在测试/调试构建里），不进界面
        let digit = ALPHABET.iter().position(|item| item == ch).expect("上面刚校验过") as u128;
        value = (value << 5) | digit;
    }
    let mut raw = [0u8; RECOVERY_BYTES];
    for slot in raw.iter_mut().rev() {
        *slot = (value & 0xff) as u8;
        value >>= 8;
    }
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 恢复码排版与归一化是互逆的() {
        let raw = [7u8; RECOVERY_BYTES];
        let code = recovery_code_from(&raw);
        assert_eq!(code.len(), RECOVERY_CHARS + 5, "24 个字符 + 5 个分隔符");
        assert_eq!(code.matches('-').count(), 5);
        assert_eq!(normalize_recovery(&code).unwrap(), raw);
    }

    #[test]
    fn 抄错的写法照样认得出() {
        // 挑一串必含 0 与 1 的死值——不然「替换了却没替到」会让这条变成空跑
        let plain = "0123456789ABCDEFGHJKMNPQ";
        let confused = "OI23456789ABCDEFGHJKMNPQ";
        assert_eq!(plain.len(), RECOVERY_CHARS);
        assert_eq!(
            normalize_recovery(plain).unwrap(),
            normalize_recovery(confused).unwrap(),
            "易混字符 O/I/L 认成 0/1"
        );
        let raw = [3u8; RECOVERY_BYTES];
        let code = recovery_code_from(&raw);
        let messy = code.to_lowercase().replace('-', " ");
        assert_eq!(normalize_recovery(&messy).unwrap(), raw, "小写 + 空格分隔");
    }

    #[test]
    fn 长度或字符不对就拒收() {
        assert!(normalize_recovery("ABCD").is_err());
        let raw = [1u8; RECOVERY_BYTES];
        let mut code = recovery_code_from(&raw);
        code.push('!');
        assert!(normalize_recovery(&code).is_err());
    }

    #[test]
    fn 口令不替作者猜_但空口令不许用() {
        assert!(Secret::Passphrase(String::new()).material().is_err());
        let spaced = Secret::Passphrase(" 有空格 ".to_string()).material().unwrap();
        assert_eq!(&spaced[..], " 有空格 ".as_bytes(), "前后空格是口令的一部分");
    }

    #[test]
    fn 两把钥匙各占各的槽() {
        assert_eq!(Secret::Passphrase("x".into()).slot(), SLOT_PASSPHRASE);
        assert_eq!(Secret::Recovery("x".into()).slot(), SLOT_RECOVERY);
    }
}
