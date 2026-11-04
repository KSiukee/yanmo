// SPDX-License-Identifier: AGPL-3.0-or-later
//! 大地图（体验版）——研墨随包示例模块的**源码**，写给想写模块的人当参考。
//!
//! 它跟完整版（收费模块）的差别只有一处：**不读正文**。所以没有「按词表在正文里找候选」
//! 那一步，也没有候选库与叩问；其余（目录 / 设定卡 / 伏笔 → 一张地图）是同一套做法。
//!
//! 三条纪律（写模块的人都该守）：
//! - **不回写核心**：只 `read`，输出只进自己的 KV（`data_put`）；
//! - **不联网**：这一份没有任何网络调用；
//! - **不猜格式**：核心给什么 JSON 就按字段读，缺字段当没有，不 panic。
//!
//! ABI（宿主 `env` 模块，稳定码）：`read(cap, arg_ptr, arg_len, out_ptr, out_cap)`、
//! `data_put(key_ptr, key_len, val_ptr, val_len)`、`emit(ptr, len)`。
//!
//! 编它：`build.ps1`（需要 `wasm32-unknown-unknown`）——产物会覆盖
//! `../bigmap-lite/module.wasm`，也就是随安装包分发的那一份。

use serde_json::{json, Value};

#[link(wasm_import_module = "env")]
extern "C" {
    fn read(cap: i32, arg_ptr: i32, arg_len: i32, out_ptr: i32, out_cap: i32) -> i32;
    fn data_put(key_ptr: i32, key_len: i32, value_ptr: i32, value_len: i32) -> i32;
    fn emit(ptr: i32, len: i32) -> i32;
}

/// 能力 id（与宿主一致，已发布别改）。
const CAP_WORKS: i32 = 1;
/// 注意：**没有 CAP_BODY**——体验版不读正文。
const CAP_OUTLINE: i32 = 4;
const CAP_ENTITIES: i32 = 6;
const CAP_FORESHADOWS: i32 = 8;

/// 输出缓冲：一次读取的结果先落这里（4 MiB；真书的目录/设定卡远小于它）。
static mut OUT: [u8; 4 * 1024 * 1024] = [0; 4 * 1024 * 1024];

/// 读一块数据。`-2` 是"缓冲区不够"；其它负数当失败。
fn read_area(cap: i32, arg: &str) -> Result<Value, i32> {
    let mut arg_buf = [0u8; 64];
    let arg_len = arg.len().min(arg_buf.len());
    arg_buf[..arg_len].copy_from_slice(&arg.as_bytes()[..arg_len]);
    unsafe {
        let ptr = std::ptr::addr_of_mut!(OUT) as *mut u8;
        let cap_len = 4 * 1024 * 1024;
        let len = read(cap, arg_buf.as_ptr() as i32, arg_len as i32, ptr as i32, cap_len);
        if len < 0 {
            return Err(len);
        }
        let slice = std::slice::from_raw_parts(ptr, len as usize);
        serde_json::from_slice(slice).map_err(|_| -3)
    }
}

fn as_array(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or(&[])
}

fn text(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

fn number(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

/// 跑一次：整理地图数据 → 存进自己的 KV → emit 摘要。
#[no_mangle]
pub extern "C" fn run() -> i32 {
    let works = match read_area(CAP_WORKS, "") {
        Ok(value) => value,
        Err(_) => return 10,
    };
    let Some(first) = as_array(&works).first() else {
        // 一本书都没有：不是错，什么都不写，如实说
        return emit_summary(json!({
            "ok": true,
            "works": 0,
            "places": 0,
            "persons": 0,
            "open_foreshadows": 0,
            "bytes": 0,
        }));
    };
    let Some(work_id) = number(first, "id") else {
        return 11;
    };
    let work_title = text(first, "title");
    let arg = work_id.to_string();

    let outline = read_area(CAP_OUTLINE, &arg).unwrap_or(json!([]));
    let entities = read_area(CAP_ENTITIES, &arg).unwrap_or(json!([]));
    let foreshadows = read_area(CAP_FORESHADOWS, &arg).unwrap_or(json!([]));

    // 目录里混着"作品根"节点（没有章节标题）：只认 `kind == chapter` 的行。
    let mut chapters: Vec<Value> = Vec::new();
    for row in as_array(&outline) {
        let node = row.get("node").cloned().unwrap_or(Value::Null);
        if text(&node, "kind") != "chapter" {
            continue;
        }
        let index = chapters.len();
        chapters.push(json!({
            "index": index,
            "id": number(&node, "id"),
            "title": text(&node, "title"),
            "lines": row.get("lines").cloned().unwrap_or(json!([])),
        }));
    }

    // 章节标题里出现过这个名字（或别名）的第一处，就是这个地点"最早露出"的位置。
    // 只认标题、不读正文：**不猜正文**，也不把"没出现在标题里"说成"不存在"。
    let chapter_titles: Vec<String> = chapters
        .iter()
        .map(|chapter| chapter["title"].as_str().unwrap_or("").to_string())
        .collect();

    let mut places = Vec::new();
    let mut persons = Vec::new();
    for card in as_array(&entities) {
        let kind = text(card, "kind");
        let name = text(card, "name");
        let aliases = card.get("aliases").cloned().unwrap_or(json!([]));
        let mut seen_in_titles: Vec<usize> = Vec::new();
        for (index, title) in chapter_titles.iter().enumerate() {
            if title.is_empty() {
                continue;
            }
            let alias_hit = aliases
                .as_array()
                .map(|list| {
                    list.iter()
                        .filter_map(Value::as_str)
                        .any(|alias| !alias.is_empty() && title.contains(alias))
                })
                .unwrap_or(false);
            if (!name.is_empty() && title.contains(&name)) || alias_hit {
                seen_in_titles.push(index);
            }
        }
        let entry = json!({
            "id": number(card, "id"),
            "name": name,
            "aliases": aliases,
            "note": text(card, "note"),
            "seen_in_titles": seen_in_titles,
        });
        if kind == "setting" {
            places.push(entry);
        } else if kind == "person" {
            persons.push(entry);
        }
    }

    let mut open = Vec::new();
    for line in as_array(&foreshadows) {
        let collected = line
            .get("collected_node")
            .map(|value| !value.is_null())
            .unwrap_or(false);
        if collected {
            continue;
        }
        open.push(json!({
            "id": number(line, "id"),
            "gist": text(line, "body"),
            "planted_node": line.get("planted_node").cloned().unwrap_or(Value::Null),
            "note": text(line, "note"),
        }));
    }

    let document = json!({
        "format": "yanmo-bigmap/1",
        "work_id": work_id,
        "work_title": work_title,
        "chapters": chapters,
        "places": places,
        "persons": persons,
        "open_foreshadows": open,
        "stats": {
            "chapters": chapters.len(),
            "places": places.len(),
            "persons": persons.len(),
            "open_foreshadows": open.len(),
        },
    });
    let Ok(payload) = serde_json::to_vec(&document) else {
        return 12;
    };
    let key = b"map.json";
    let stored = unsafe {
        data_put(
            key.as_ptr() as i32,
            key.len() as i32,
            payload.as_ptr() as i32,
            payload.len() as i32,
        )
    };
    if stored != 0 {
        return 13;
    }

    emit_summary(json!({
        "ok": true,
        "works": as_array(&works).len(),
        "places": document["stats"]["places"],
        "persons": document["stats"]["persons"],
        "open_foreshadows": document["stats"]["open_foreshadows"],
        "bytes": payload.len(),
    }))
}

fn emit_summary(summary: Value) -> i32 {
    let Ok(text) = serde_json::to_vec(&summary) else {
        return 14;
    };
    unsafe {
        emit(text.as_ptr() as i32, text.len() as i32);
    }
    0
}
