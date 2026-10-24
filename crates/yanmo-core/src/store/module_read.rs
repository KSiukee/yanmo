//! 模块读核心数据的**唯一门卫**：按授予逐项放行，**只读**。
//!
//! # 为什么要在核心侧再做一层门卫
//!
//! 能力清单写在模块清单里只是"它想要什么"；真正能不能读，得看注册表里**用户给了什么**、
//! 模块**有没有被冻结**。这一层把这两件事合起来判，并且只暴露读方法——
//! 模块拿不到 `Store` 本身，也就拿不到任何写路径。
//!
//! # 读什么、读不到什么
//!
//! 八个区域与清单里的 [`CoreDataArea`] 一一对应：作品 / 目录 / 正文 / 大纲 / 碎片 /
//! 设定卡 / 场景卡 / 伏笔。**跨模块读**不在这里（那是模块数据区之间的事，
//! 走声明的数据契约）；**写核心**永远没有——这是模块地基的硬边界。
//!
//! 正文按**整章**给（协议原则：不提供逐字符接口）；碎片最多一次一万条
//! （再多请让模块自己按时间分批要，别把一本书的碎片池一口吞进内存）。

use super::{NodeSummary, ShelfEntry, Store};
use crate::error::{codes, Error, Result};
use crate::model::{
    EntityCard, Foreshadow, Fragment, ModuleRecord, SceneFields, StoryLine,
};
use yanmo_proto::capability::CoreDataArea;

/// 一次最多给多少条碎片（见模块文档：批量级，不逐条）。
const MAX_FRAGMENTS: usize = 10_000;

/// 大纲的一行：目录条目（自带一句话）+ 它推进的故事线。
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleOutlineRow {
    pub node: NodeSummary,
    pub lines: Vec<StoryLine>,
}

/// 一个模块的只读视图（**绑定它的注册表记录**）。
pub struct ModuleReadApi<'a> {
    store: &'a Store,
    record: ModuleRecord,
}

impl std::fmt::Debug for ModuleReadApi<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModuleReadApi")
            .field("module_id", &self.record.id())
            .field("enabled", &self.record.enabled)
            .finish_non_exhaustive()
    }
}

impl Store {
    /// 给一个模块开只读视图；没装过 → `module.not_found`。
    ///
    /// 视图持有的是注册表记录的**快照**：同一次调用里授予/冻结不会中途变。
    pub fn module_reader(&self, module_id: &str) -> Result<ModuleReadApi<'_>> {
        Ok(ModuleReadApi { store: self, record: self.module_record(module_id)? })
    }
}

impl<'a> ModuleReadApi<'a> {
    pub fn module_id(&self) -> &str {
        self.record.id()
    }

    /// 这次视图对应的注册表记录（冻结 / 授予 / 认证状态都在里面）。
    pub fn record(&self) -> &ModuleRecord {
        &self.record
    }

    /// 门卫：**先看冻结，再看授予**。
    ///
    /// 两个原因分开报：冻结是"整个模块被停用"，没授予是"这一项没同意"——
    /// 界面要说的话不一样。
    fn allow(&self, area: CoreDataArea) -> Result<()> {
        if !self.record.enabled {
            return Err(Error::invalid_with(
                codes::MODULE_DISABLED,
                [("module_id", self.record.id().to_string())],
            ));
        }
        if !self.record.granted.read.contains(&area) {
            return Err(Error::invalid_with(
                codes::MODULE_CAPABILITY_DENIED,
                [("capability", area.as_str().to_string())],
            ));
        }
        Ok(())
    }

    /// 作品列表（书架口径：不含回收站）。
    pub fn works(&self) -> Result<Vec<ShelfEntry>> {
        self.allow(CoreDataArea::Works)?;
        self.store.shelf()
    }

    /// 目录树（元数据，**不含正文**）。
    pub fn tree(&self, work_id: i64) -> Result<Vec<NodeSummary>> {
        self.allow(CoreDataArea::Tree)?;
        self.store.list_nodes(work_id)
    }

    /// 一章的正文全文。
    pub fn body(&self, node_id: i64) -> Result<String> {
        self.allow(CoreDataArea::Body)?;
        self.store.read_body(node_id)
    }

    /// 大纲：每一段的一句话 + 它推进的故事线（按目录顺序）。
    pub fn outline(&self, work_id: i64) -> Result<Vec<ModuleOutlineRow>> {
        self.allow(CoreDataArea::Outline)?;
        let nodes = self.store.list_nodes(work_id)?;
        let lines = self.store.node_lines(work_id)?;
        Ok(nodes
            .into_iter()
            .map(|node| ModuleOutlineRow {
                lines: lines.get(&node.id).cloned().unwrap_or_default(),
                node,
            })
            .collect())
    }

    /// 碎片池（灵感 / 事件 / 口述 / 问题 / 答案；不含已删的）。
    pub fn fragments(&self, work_id: i64) -> Result<Vec<Fragment>> {
        self.allow(CoreDataArea::Fragments)?;
        self.store.recent_fragments(work_id, MAX_FRAGMENTS)
    }

    /// 设定卡（人物 / 设定）。
    pub fn entities(&self, work_id: i64) -> Result<Vec<EntityCard>> {
        self.allow(CoreDataArea::Entities)?;
        self.store.entity_cards(work_id, None)
    }

    /// 场景卡四格（只有填过四格的节点才会出现）。
    pub fn scenes(&self, work_id: i64) -> Result<Vec<(i64, String, SceneFields)>> {
        self.allow(CoreDataArea::Scenes)?;
        self.store.nodes_with_fields(work_id)
    }

    /// 伏笔账本。
    pub fn foreshadows(&self, work_id: i64) -> Result<Vec<Foreshadow>> {
        self.allow(CoreDataArea::Foreshadows)?;
        self.store.foreshadows(work_id, None)
    }
}
