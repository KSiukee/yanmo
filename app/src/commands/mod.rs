//! 壳层命令：**按域拆文件**，每个命令只做「参数转换 + 转交核心」。
//!
//! # 纪律
//!
//! - 命令粒度守则：宁可少而清晰，也别做成上帝 command；一个命令对应一个明确的用户意图。
//! - 数据权威在 [`crate::storage::AppData`]：命令**一律不接受路径参数**，界面也无从指定文件。
//!   （例外只有一处：[`restore`] 的"从这份备份恢复"——那份来源是作者点出来的，
//!   不是界面自己拼的路径；真正动文件的仍是壳与核心。**新功能一律走 [`location`] 那条更严的形态**：
//!   选中的路径只记在壳里，界面只说"搬吧"。）
//! - 按域分文件（[`system`] / [`editor`] / [`tree`] / [`appearance`] / [`typeset`] / [`compile`] / [`snapshot`] / [`work`] / [`trash`] / [`backup`] / [`restore`] / [`location`] / [`writing`]），域之间不互相调用——要复用的逻辑下沉进核心。

pub mod appearance;
/// 多处备份：备份、体检、备份列表与设置。
/// 作者信息：投稿时随稿子走的那点身份（笔名/真名/联系方式/简历；敏感项默认不写进稿件）。
pub mod author;
pub mod backup;
/// 编译：投稿版 docx / 分章 txt / 合并 txt。
pub mod compile;
/// 章长：这一本书每一章多少字、哪一章偏长（**只读**；判据与叩问那条同源）。
pub mod chapter_length;
/// 短篇集：引用聚合（往集里加篇 / 拿掉 / 重排 / 定目标 / 一键建集），**从不碰正文**。
pub mod collection;
/// 编辑器：打开章节、落盘、光标与检索。
pub mod editor;
/// 设定卡：人物与设定（大纲冲突检测的数据源之一）。
pub mod entity;
/// 伏笔：埋下的一条线头（埋/收/不写了）。
pub mod foreshadow;
/// 创作流：碎片池的记 / 看 / 删 / 捞回（**作者自己记下的东西**）。
pub mod fragment;
/// 灵感：把碎片按引力排出来，并采纳 / 丢弃（**机制排，不生成字**）。
pub mod inspire;
/// 五线推进度：**这一段在推进哪几条线**的读与写（卷与场景卡标不了，由核心拒）。
pub mod lines;
/// 命令登记处：壳上所有命令的唯一一张清单（入口只接上它）。
pub mod registry;
/// 稿子放哪：位置查询、换位置、打开文件夹。
pub mod location;
/// 稿件台：按状态看稿子、记一次投稿、改状态（一稿多投只拦不禁止）。
pub mod manuscript;
/// 磁盘即 `.md`：强镜像的状态、开关、立即同步与打开目录。
pub mod mirror;
/// 大纲体检：把对不上的地方列出来（只报告，不改稿）。
pub mod outline;
/// 叩问：问题候选取材、处置四件套与记灵感（**只问不写**）。
pub mod question;
/// 叩问的「推」：什么时候主动开口（配额 / 冷却 / 时机），**与面板那一族分开**。
pub mod question_push;
/// 从备份恢复：预览、体检、换库。
pub mod restore;
/// 场景卡：四格（视角 / 目标 / 冲突 / 结果）的读写。
/// 加密备份包：开关、口令（只在内存）、恢复码。
pub mod seal;
/// 检索：在正文里找一句话（命中带书名与片段；检索语义全在核心）。
pub mod search;
pub mod scene;
/// 会话与退出闸门：心跳 / 上次会话的交代 / 关窗回话与收尾（**壳自己的那一族**）。
pub mod session;
/// 流程（SOP）：作者自己的写作流程（两层继承，**不强制顺序、不打卡**）。
pub mod sop;
/// 每章版本快照：列表、对照、回滚。
pub mod snapshot;
/// 拆章：在作者指定的切点把一章切成两章，以及撤销那一次（**只由作者触发**）。
pub mod split;
/// 系统信息：版本、数据目录、验收与诊断。
pub mod system;
/// 回收站：列出、预览、恢复。
pub mod trash;
/// 目录树：读取，以及建 / 改名 / 移动 / 删除。
pub mod tree;
/// 排版清理：预览与执行。
pub mod typeset;
/// 分卷：卷长口径、收卷提议、成卷与撤卷。
pub mod volume;
/// 书架：列表、新建、改名、删除、导出与编译入口。
pub mod work;
/// 码字账本：今日进度、日历、每日目标。
pub mod writing;
