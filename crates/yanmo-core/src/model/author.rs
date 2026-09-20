//! 作者信息：**投稿时随稿子走的那点身份**（笔名 / 真名 / 联系方式 / 文学简历）。
//!
//! # 为什么是这几项、放在文末
//!
//! 2026-09-20 调研了两类惯例：文学期刊要**文末附作者简介**（《胶东文学》征稿启事原文列的
//! 就是"笔名、真名、电话、身份证号、银行账号及开户行名、通联地址及邮编、文学简历"）；
//! 学术期刊是标题下方署名 + 首页脚注。研墨服务的是投稿写作者，所以这一份**只往文末走**。
//!
//! # 为什么跟人、不跟书
//!
//! 电话、地址、简历这些半年也不会变，每投一篇重填一遍是折磨。所以它像外观偏好一样
//! 只有**一份全局的**（`settings` 的 `author` 键），所有作品共用。
//!
//! # 敏感项默认不写进稿件
//!
//! 身份证号与银行账号是**给稿费结算用的**，不该跟着每一份投稿文件到处跑。
//! 所以它们单独一栏、单独一个开关：**默认不写**——要投那家要这些的刊物时，作者自己打开。

/// 作者信息（**稀疏**：`None` = 没设过，`Some("")` = 显式清空）。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct Author {
    /// 笔名（发表时用的名字）
    pub pen_name: Option<String>,
    /// 真名（有的刊物要"笔名 + 真名"并排）
    pub real_name: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    /// 通联地址
    pub address: Option<String>,
    pub postcode: Option<String>,
    /// 文学简历（自由文本，几行）
    pub bio: Option<String>,
    /// **敏感**：身份证号
    pub id_number: Option<String>,
    /// **敏感**：银行账号与开户行
    pub bank: Option<String>,
    /// 投稿时附不附作者简介（`None` = 没设过，按默认**不附**）
    pub attach: Option<bool>,
    /// 附的时候带不带身份证与银行账号（`None` = 没设过，按默认**不带**）
    pub with_sensitive: Option<bool>,
}

/// 落到具体值的那一份（读取时把稀疏层补齐；**坏记录当没设过**）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAuthor {
    pub pen_name: String,
    pub real_name: String,
    pub phone: String,
    pub email: String,
    pub address: String,
    pub postcode: String,
    pub bio: String,
    pub id_number: String,
    pub bank: String,
    pub attach: bool,
    pub with_sensitive: bool,
}

/// 真正要写进稿件的那几项（空的不写；敏感项按开关决定在不在）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuthorBlock {
    pub pen_name: String,
    pub real_name: String,
    pub phone: String,
    pub email: String,
    pub address: String,
    pub postcode: String,
    pub bio: String,
    pub id_number: String,
    pub bank: String,
}

impl Default for ResolvedAuthor {
    fn default() -> Self {
        Self::empty()
    }
}

impl ResolvedAuthor {
    /// 一条都没填的那一份。
    pub fn empty() -> Self {
        Self {
            pen_name: String::new(),
            real_name: String::new(),
            phone: String::new(),
            email: String::new(),
            address: String::new(),
            postcode: String::new(),
            bio: String::new(),
            id_number: String::new(),
            bank: String::new(),
            attach: false,
            with_sensitive: false,
        }
    }

    /// 有没有可写的身份信息（一个字都没填就是没有）。
    pub fn has_content(&self) -> bool {
        !(self.pen_name.is_empty()
            && self.real_name.is_empty()
            && self.phone.is_empty()
            && self.email.is_empty()
            && self.address.is_empty()
            && self.postcode.is_empty()
            && self.bio.is_empty()
            && self.id_number.is_empty()
            && self.bank.is_empty())
    }

    /// **要写进稿件的那些**：没勾"附"、或一个字都没填 → `None`。
    ///
    /// 没勾"带敏感项"时，身份证号与银行账号在这里就被清掉——
    /// 让"不该带的东西"在离开核心之前就不存在，比在下游各处记得过滤可靠。
    pub fn block(&self) -> Option<AuthorBlock> {
        if !self.attach || !self.has_content() {
            return None;
        }
        let (id_number, bank) = if self.with_sensitive {
            (self.id_number.clone(), self.bank.clone())
        } else {
            (String::new(), String::new())
        };
        Some(AuthorBlock {
            pen_name: self.pen_name.clone(),
            real_name: self.real_name.clone(),
            phone: self.phone.clone(),
            email: self.email.clone(),
            address: self.address.clone(),
            postcode: self.postcode.clone(),
            bio: self.bio.clone(),
            id_number,
            bank,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filled() -> ResolvedAuthor {
        ResolvedAuthor {
            pen_name: "南岸".into(),
            phone: "13800000000".into(),
            id_number: "110101199001010011".into(),
            bank: "工商银行 6222…".into(),
            ..ResolvedAuthor::empty()
        }
    }

    #[test]
    fn nothing_filled_means_nothing_to_attach() {
        let mut author = ResolvedAuthor::empty();
        author.attach = true;
        assert!(!author.has_content());
        assert_eq!(author.block(), None);
    }

    #[test]
    fn attach_off_keeps_it_out_of_the_document() {
        assert_eq!(filled().block(), None, "默认不附：一个字节都不写");
    }

    #[test]
    fn attach_on_includes_it_but_leaves_the_sensitive_two_out() {
        let mut author = filled();
        author.attach = true;
        let block = author.block().unwrap();
        assert_eq!(block.pen_name, "南岸");
        assert!(block.id_number.is_empty() && block.bank.is_empty(), "敏感项默认不写");
        author.with_sensitive = true;
        let block = author.block().unwrap();
        assert_eq!(block.id_number, "110101199001010011", "勾了才带");
    }
}
