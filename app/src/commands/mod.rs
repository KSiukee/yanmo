//! 壳层命令：**按域拆文件**，每个命令只做「参数转换 + 转交核心」。
//!
//! # 纪律
//!
//! - 命令粒度守则：宁可少而清晰，也别做成上帝 command；一个命令对应一个明确的用户意图。
//! - 数据权威在 [`crate::storage::AppData`]：命令**一律不接受路径参数**，界面也无从指定文件。
//!   （例外只有一处：[`restore`] 的"从这份备份恢复"——那份来源是作者点出来的，
//!   不是界面自己拼的路径；真正动文件的仍是壳与核心。）
//! - 按域分文件（[`system`] / [`editor`] / [`tree`] / [`gap`] / [`appearance`] / [`snapshot`] / [`work`] / [`trash`] / [`backup`] / [`restore`]），域之间不互相调用——要复用的逻辑下沉进核心。

pub mod appearance;
pub mod backup;
pub mod editor;
pub mod gap;
pub mod restore;
pub mod snapshot;
pub mod system;
pub mod trash;
pub mod tree;
pub mod work;
