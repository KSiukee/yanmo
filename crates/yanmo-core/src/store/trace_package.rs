// i18n-allow-file: verify_trace_events 返回的错误文案给测试与命令行看，不进界面（界面字典不分发到包外）
//! 创作留痕自证包：**把"过程"打包成可以自己验的一组文件**。
//!
//! # 它证明什么、不证明什么
//!
//! 证明：这本书的**操作记录、版本快照、来源分布、口述/问答痕迹**是**这一份**，
//! 而且从第一条到最后一条用**链式哈希**串起来——事后改任何一条，后面全对不上。
//!
//! 不证明：不能证明"这稿是我写的"（AI 生成再整段粘贴也会留下一条记录，只是那条记录
//! 会显示"一次到位、没有中间态"）；也不能证明**本机时间**没被改过（时间戳来自本机时钟，
//! 作者自己可以调系统时间；要更强的时间锚得接外部服务，那与零出网冲突，本包默认不接）。
//!
//! # 包里的文件
//!
//! - `manifest.json`：口径、计数、链头哈希、每个文件的 SHA-256；
//! - `events.jsonl`：按时间排的事件流（每条带 `prev` 与 `digest`，一行一条）；
//! - `sources.json`：每章"字从哪儿来"（只给分档字数，**不含正文**）；
//! - `versions.json`：每章的版本快照（时间 / 字数 / 理由 / 是否亲手留存）；
//! - `README.txt`：给不装研墨的人看的一页说明（能证明 / 不能证明 / 怎么自己验）。
//!
//! 隐私分寸：**包里没有正文**——事件只带摘要与计数，来源只带字数。要连正文一起交出去，
//! 那是"交稿"（导出）的事，不是这份"过程材料"的事。

use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use super::Store;
use crate::error::Result;

/// 包格式版本（以后加字段要涨它，读包的人据此判断）。
pub const TRACE_PACKAGE_VERSION: u32 = 1;

/// 一个要写进包里的文件（名字 + 字节）。壳负责落到磁盘，核心不碰路径。
pub struct TraceFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// 打好的包：文件都在内存里，壳拿去写。
pub struct TracePackage {
    pub work_id: i64,
    pub title: String,
    /// 链头（最后一条的 digest）——与 `manifest.json` 里写的是同一个值
    pub head: String,
    pub events: usize,
    pub files: Vec<TraceFile>,
}

/// 事件流里的一条（字段顺序就是序列化顺序，**别重排**：重排会让旧包验不过）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEvent {
    pub seq: u64,
    pub at: i64,
    pub kind: String,
    pub subject: String,
    pub detail: serde_json::Value,
    pub prev: String,
    pub digest: String,
}

/// 链上算哈希的那一份（不含 `digest` 自己，也不含 `prev`——`prev` 单独拼在前面）。
#[derive(Serialize)]
struct ChainPayload<'a> {
    seq: u64,
    at: i64,
    kind: &'a str,
    subject: &'a str,
    detail: &'a serde_json::Value,
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn chain_digest(prev: &str, event: &TraceEvent) -> String {
    let payload = ChainPayload {
        seq: event.seq,
        at: event.at,
        kind: &event.kind,
        subject: &event.subject,
        detail: &event.detail,
    };
    let body = serde_json::to_vec(&payload).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(prev.as_bytes());
    hasher.update(b"\n");
    hasher.update(&body);
    format!("{:x}", hasher.finalize())
}

/// 自己验一遍事件流（不装研墨也能用：任何语言照着重算即可）。
///
/// 返回 `(条数, 链头)`；断链 / 坏行 / 序号不连续都返回 `Err(说明)`。
pub fn verify_trace_events(jsonl: &str) -> std::result::Result<(usize, String), String> {
    let mut prev = "genesis".to_string();
    let mut count = 0usize;
    for (index, line) in jsonl.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let event: TraceEvent = serde_json::from_str(line)
            .map_err(|error| format!("第 {} 行不是合法事件：{error}", index + 1))?;
        if event.seq != count as u64 + 1 {
            return Err(format!("第 {} 行的 seq 是 {}，应当是 {}", index + 1, event.seq, count + 1));
        }
        if event.prev != prev {
            return Err(format!("第 {} 行的 prev 与上一条的 digest 对不上（断链）", index + 1));
        }
        let digest = chain_digest(&prev, &event);
        if digest != event.digest {
            return Err(format!("第 {} 行的 digest 对不上（内容被改过）", index + 1));
        }
        prev = digest;
        count += 1;
    }
    Ok((count, prev))
}

/// 收哪些事件：`All` = 全量（默认）；`Creative` = 只收创作相关，滤掉性能诊断。
///
/// 为什么要有这个开关：`op.slow_command` / `op.lag` 是**这台机器当时的性能读数**，
/// 交给编辑 / 平台看会显得杂（"pick_font 10 秒"跟稿子没有关系）。默认仍收全量——
/// 少收是作者主动选的，不是我们替他藏。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TraceScope {
    #[default]
    All,
    Creative,
}

impl TraceScope {
    pub fn parse(value: &str) -> Self {
        if value == "creative" {
            Self::Creative
        } else {
            Self::All
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Creative => "creative",
        }
    }

    fn keeps(self, kind: &str) -> bool {
        match self {
            Self::All => true,
            Self::Creative => !matches!(kind, "op.slow_command" | "op.lag"),
        }
    }
}

/// 一条待排序的原始事件（`at` 相同就按来源顺序稳定排）。
struct RawEvent {
    at: i64,
    kind: String,
    subject: String,
    detail: serde_json::Value,
    tie: usize,
}

impl Store {
    /// 打一份这本书的留痕包。**只读**：不改库、不写文件。
    pub fn build_trace_package(&self, work_id: i64) -> Result<TracePackage> {
        self.build_trace_package_with(work_id, TraceScope::All)
    }

    /// 同上，但可以选收哪些事件（见 [`TraceScope`]）。
    pub fn build_trace_package_with(
        &self,
        work_id: i64,
        scope: TraceScope,
    ) -> Result<TracePackage> {
        let work = self.get_work(work_id)?;
        let nodes = self.list_nodes(work_id)?;
        let node_ids: Vec<i64> = nodes.iter().map(|node| node.id).collect();
        let node_titles: std::collections::HashMap<i64, String> = nodes
            .iter()
            .map(|node| {
                // 显示标题：模板（`第{$N}章`）要渲染成人能读的样子——包里是给外人看的，
                // 摆一个模板等于把内部写法泄给他。普通标题就原样，省一次查询。
                let title = if node.title.contains("{$") {
                    self.rendered_title(node.id).unwrap_or_else(|_| node.title.clone())
                } else {
                    node.title.clone()
                };
                (node.id, title)
            })
            .collect();

        let mut raw: Vec<RawEvent> = Vec::new();
        let mut tie = 0usize;
        let mut push = |at: i64, kind: String, subject: String, detail: serde_json::Value| {
            raw.push(RawEvent { at, kind, subject, detail, tie });
            tie += 1;
        };

        // ① 操作留痕：只取与这本书有关的实体
        if let Ok(mut stmt) = self.conn.prepare(
            "SELECT op, entity, entity_id, payload, created_at FROM op_log ORDER BY seq",
        ) {
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            });
            if let Ok(rows) = rows {
                for row in rows.flatten() {
                    let (op, entity, entity_id, payload, created_at) = row;
                    let mine = (entity == "works" && entity_id == work_id)
                        || node_ids.contains(&entity_id);
                    if !mine {
                        continue;
                    }
                    let detail = serde_json::from_str(&payload).unwrap_or(json!({ "raw": payload }));
                    let kind = format!("op.{op}");
                    if !scope.keeps(&kind) {
                        continue;
                    }
                    push(created_at, kind, format!("{entity}:{entity_id}"), detail);
                }
            }
        }

        // ② 版本快照：每章各有多少个、什么时候、多少字
        for node_id in &node_ids {
            let title = node_titles.get(node_id).cloned().unwrap_or_default();
            for snapshot in self.list_snapshots(*node_id)? {
                push(
                    snapshot.created_at,
                    "snapshot".to_string(),
                    format!("node:{node_id}"),
                    json!({
                        "title": title,
                        "chars": snapshot.char_count,
                        "reason": snapshot.reason,
                        "pinned": snapshot.pinned,
                    }),
                );
            }
        }

        // ③ 口述 / 问答 / 灵感：只记"记下了什么类型、多长、从哪来"，不记正文
        if let Ok(mut stmt) = self.conn.prepare(
            "SELECT frag_kind, source, length(body), created_at, derived_from
               FROM fragments WHERE work_id = ?1 AND deleted_at IS NULL
               ORDER BY created_at, id",
        ) {
            let rows = stmt.query_map([work_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                ))
            });
            if let Ok(rows) = rows {
                for row in rows.flatten() {
                    let (kind, source, chars, created_at, derived_from) = row;
                    push(
                        created_at,
                        format!("fragment.{kind}"),
                        "fragments".to_string(),
                        json!({ "source": source, "chars": chars, "derived_from": derived_from }),
                    );
                }
            }
        }

        raw.sort_by_key(|event| (event.at, event.tie));

        // ④ 串链
        let mut events: Vec<TraceEvent> = Vec::with_capacity(raw.len());
        let mut prev = "genesis".to_string();
        for item in raw {
            let mut event = TraceEvent {
                seq: events.len() as u64 + 1,
                at: item.at,
                kind: item.kind,
                subject: item.subject,
                detail: item.detail,
                prev: prev.clone(),
                digest: String::new(),
            };
            event.digest = chain_digest(&prev, &event);
            prev = event.digest.clone();
            events.push(event);
        }
        let head = prev;
        let jsonl = events
            .iter()
            .map(|event| serde_json::to_string(event).unwrap_or_default())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";

        // ⑤ 来源分布（每章一档字数，不含正文）
        let mut chapters = Vec::new();
        for node in &nodes {
            let breakdown = self.source_breakdown(node.id)?;
            if breakdown.shares.is_empty() {
                continue;
            }
            chapters.push(json!({
                "node_id": node.id,
                "title": node_titles.get(&node.id).cloned().unwrap_or_default(),
                "kind": node.kind.as_str(),
                "shares": breakdown.shares,
            }));
        }

        // ⑥ 版本清单（按章聚合，给"版本演进"一眼看）
        let mut versions = Vec::new();
        for node_id in &node_ids {
            let list = self.list_snapshots(*node_id)?;
            if list.is_empty() {
                continue;
            }
            versions.push(json!({
                "node_id": node_id,
                "title": node_titles.get(node_id).cloned().unwrap_or_default(),
                "count": list.len(),
                "snapshots": list.iter().map(|snapshot| json!({
                    "id": snapshot.id,
                    "at": snapshot.created_at,
                    "chars": snapshot.char_count,
                    "reason": snapshot.reason,
                    "pinned": snapshot.pinned,
                })).collect::<Vec<_>>(),
            }));
        }

        let readme = super::trace_readme::render_readme(
            &work.title,
            events.len(),
            &head,
            chapters.len(),
            scope,
        );
        let sources = serde_json::to_vec_pretty(&json!({
            "package_version": TRACE_PACKAGE_VERSION,
            "work_id": work_id,
            "chapters": chapters,
        }))
        .unwrap_or_default();
        let versions = serde_json::to_vec_pretty(&json!({
            "package_version": TRACE_PACKAGE_VERSION,
            "work_id": work_id,
            "nodes": versions,
        }))
        .unwrap_or_default();

        let mut files = vec![
            TraceFile { name: "events.jsonl".to_string(), bytes: jsonl.into_bytes() },
            TraceFile { name: "sources.json".to_string(), bytes: sources },
            TraceFile { name: "versions.json".to_string(), bytes: versions },
            TraceFile { name: "README.txt".to_string(), bytes: readme.into_bytes() },
            // 自带的校验脚本：包外的人不用写代码，`python verify.py` 就能验
            TraceFile {
                name: "verify.py".to_string(),
                bytes: super::trace_verify::VERIFY_PY.as_bytes().to_vec(),
            },
        ];
        let manifest = serde_json::to_vec_pretty(&json!({
            "package_version": TRACE_PACKAGE_VERSION,
            "engine_version": env!("CARGO_PKG_VERSION"),
            "created_at": crate::time::now_millis(),
            "scope": scope.as_str(),
            "work": { "id": work_id, "title": work.title, "kind": work.kind.as_str(), "language": work.language.as_str() },
            "event_count": events.len(),
            "chain_head": head,
            "chain_rule": "digest = sha256(prev + '\\n' + compact_json({seq,at,kind,subject,detail}))",
            "files": files.iter().map(|file| json!({
                "name": file.name,
                "bytes": file.bytes.len(),
                "sha256": sha256_hex(&file.bytes),
            })).collect::<Vec<_>>(),
        }))
        .unwrap_or_default();
        files.push(TraceFile { name: "manifest.json".to_string(), bytes: manifest });

        Ok(TracePackage { work_id, title: work.title, head, events: events.len(), files })
    }
}
