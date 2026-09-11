//! 壳层命令：**按域拆文件**，每个命令只做「参数转换 + 转交核心」。
//!
//! # 纪律
//!
//! - 命令粒度守则：宁可少而清晰，也别做成上帝 command；一个命令对应一个明确的用户意图。
//! - 数据权威在 [`crate::storage::AppData`]：命令**一律不接受路径参数**，界面也无从指定文件。
//! - 按域分文件（[`system`] / [`editor`] / [`tree`] / [`work`]），域之间不互相调用——要复用的逻辑下沉进核心。

pub mod editor;
pub mod system;
pub mod tree;
pub mod work;
