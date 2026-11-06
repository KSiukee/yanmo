//! 留痕包的**链**：事件形状、哈希规则、独立校验。
//!
//! 单独成文件的两条理由：
//! - 链规则是**对外承诺**（README 里写着、包外的人照着算），改它比改打包逻辑更慎重；
//! - 它自带错误文案（"第几行断链"），与打包逻辑的变化理由不同。
// i18n-allow-file: 这些错误文案给测试与命令行看，不进界面（界面字典不分发到包外）
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub(super) fn chain_digest(prev: &str, event: &TraceEvent) -> String {
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
            return Err(format!(
                "第 {} 行的 seq 是 {}，应当是 {}",
                index + 1,
                event.seq,
                count + 1
            ));
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
