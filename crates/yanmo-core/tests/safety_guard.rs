//! 危险写路径的机械守卫：**删稿、清表、无 WHERE 的改写，一律要过白名单**。
//!
//! 为什么要有它：发布之后，一个编译能过、但把「删一章」写成「删一张表」的 bug，
//! 就足以把作者的稿子搞炸。虽然有备份能救回来，但信任会掉。
//! 所以这类写路径不许"顺手写出来"——每一个都必须在下面白名单里**逐条写清它为什么不伤稿件**。
//!
//! 四条规矩：
//! 1. `DROP TABLE` 一律禁止（`temp.` 上的探测表除外）；
//! 2. `DELETE FROM` 稿件表（`nodes` / `node_contents` / `works` / `snapshots`）要白名单；
//! 3. `DELETE` 或 `UPDATE ... SET` **不带 WHERE** 要白名单（upsert 的 `ON CONFLICT ... DO UPDATE` 不算）；
//! 4. `remove_dir_all(` 要白名单（理由必须说清"删的是自己造的临时/暂存目录"）。
//!
//! 白名单是**棘轮**：条目只许减不许增，且带条数——多一处、少一处都会红
//! （少了说明代码变了没人管，多了说明白名单过期了，两种都要人来重新看一眼）。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// 稿件本体：这些表里的行一旦没了，作者的劳动就没了。
const PRECIOUS: &[&str] = &["nodes", "node_contents", "works", "snapshots"];

/// 允许的例外。**每条都要写清理由**——这是"人看过一遍"的凭据，不是形式。
struct Allowance {
    file: &'static str,
    rule: &'static str,
    count: usize,
    why: &'static str,
}

const ALLOWED: &[Allowance] = &[
    Allowance {
        file: "crates/yanmo-core/src/store/trash.rs",
        rule: "delete-precious",
        count: 2,
        why: "回收站「彻底删除」的正规出口：purge_node 按子树 id 列表删、purge_work 按作品 id 删，\
              都由作者显式确认后调用，且都带 WHERE（不是全表删）",
    },
    Allowance {
        file: "crates/yanmo-core/src/store/snapshot.rs",
        rule: "delete-precious",
        count: 2,
        why: "快照的两条正常出口：删「作者指定那一版」（WHERE id = ?1）与滚动保留剪枝\
              （WHERE node_id = ?1 AND pinned = 0 且带 LIMIT，只清未钉住的自动快照）——都不碰正文",
    },
    Allowance {
        file: "crates/yanmo-core/src/store/search.rs",
        rule: "delete-without-where",
        count: 1,
        why: "重建**检索索引**（node_fts 是索引表，不是稿件）：全表清空后立刻按库里的正文重建，\
              稿件在 nodes / node_contents 里一个字都没动",
    },
    Allowance {
        file: "crates/yanmo-core/src/store/backup.rs",
        rule: "remove-dir-all",
        count: 4,
        why: "只删备份的**暂存区**与自己产出的历史备份包（都是本模块刚创建的目录）；\
              数据目录本身从不经这里删——换库留底走 restore_swap 的改名，不删",
    },
    Allowance {
        file: "app/src/acceptance.rs",
        rule: "remove-dir-all",
        count: 2,
        why: "验收模式清自己的沙箱：删之前先过 wipe_guard —— 目录要么落在系统临时目录之下、\
              要么带沙箱记号文件；**目录里有稿库又没有记号时一律拒绝**（`--dir` 参数打错也删不掉真稿库）。\
              另一处删的是沙箱内部刚建的备份落点",
    },
];

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <root>/crates/yanmo-core
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("工作区根目录取不到")
        .to_path_buf()
}

/// 要扫的源码目录（生产代码；测试里造临时库、删临时目录是正当事，不在这条守卫的范围内）。
const SCAN_DIRS: &[&str] = &[
    "crates/yanmo-core/src",
    "crates/yanmo-cli/src",
    "crates/yanmo-proto/src",
    "app/src",
];

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rs_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// 源码里的**字符串字面量**（SQL 都写在字面量里；顺带跳过注释，免得注释里的示例误报）。
fn string_literals(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut escaped = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_string {
            if escaped {
                current.push(ch);
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                out.push(std::mem::take(&mut current));
                in_string = false;
            } else {
                current.push(ch);
            }
            continue;
        }
        // 不在字面量里：`//` 之后到行尾、`/* */` 之间都当注释跳过
        if ch == '/' && chars.peek() == Some(&'/') {
            for next in chars.by_ref() {
                if next == '\n' {
                    break;
                }
            }
        } else if ch == '/' && chars.peek() == Some(&'*') {
            let mut prev = '\0';
            for next in chars.by_ref() {
                if prev == '*' && next == '/' {
                    break;
                }
                prev = next;
            }
        } else if ch == '"' {
            current.clear();
            in_string = true;
        }
    }
    out
}

/// 一个字面量里可能塞了几条语句（触发器体），按 `;` 切开、压平空白再逐条看。
fn statements(literal: &str) -> Vec<String> {
    literal
        .split(';')
        .map(|part| part.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|part| !part.is_empty())
        .collect()
}

/// 抓出所有违规：(规则名, 人话描述)。
fn violations(relative: &str, text: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for literal in string_literals(text) {
        for statement in statements(&literal) {
            let upper = statement.to_uppercase();
            let words: Vec<&str> = statement.split_whitespace().collect();

            // ① DROP TABLE（`temp.` 上的探测表是能力检测，不是稿件）
            if let Some(at) = upper.find("DROP TABLE") {
                let _ = at;
                if let Some(table) = words.iter().position(|word| word.eq_ignore_ascii_case("TABLE")) {
                    let target = words.get(table + 1).copied().unwrap_or("");
                    if !target.to_ascii_lowercase().starts_with("temp.") {
                        found.push(("drop-table".into(), statement.clone()));
                    }
                }
            }

            // ② DELETE FROM 稿件表 / ③ 不带 WHERE 的 DELETE
            if let Some(at) = upper.find("DELETE FROM") {
                let _ = at;
                if let Some(table) = words.iter().position(|word| word.eq_ignore_ascii_case("FROM")) {
                    let target = words
                        .get(table + 1)
                        .copied()
                        .unwrap_or("")
                        .trim_matches(|c: char| !c.is_alphanumeric() && c != '_')
                        .to_ascii_lowercase();
                    if PRECIOUS.contains(&target.as_str()) {
                        found.push(("delete-precious".into(), statement.clone()));
                    }
                }
                if !upper.contains(" WHERE ") {
                    found.push(("delete-without-where".into(), statement.clone()));
                }
            }

            // ④ 不带 WHERE 的 UPDATE（upsert 的 `ON CONFLICT ... DO UPDATE SET` 不算）
            if upper.contains("UPDATE ") && upper.contains(" SET ") && !upper.contains("ON CONFLICT") {
                if !upper.contains(" WHERE ") {
                    found.push(("update-without-where".into(), statement.clone()));
                }
            }
        }
    }

    // ⑤ 直接删目录（代码层，不在字面量里）
    let dir_deletes = text.matches("remove_dir_all(").count();
    for _ in 0..dir_deletes {
        found.push(("remove-dir-all".into(), "remove_dir_all(…)".into()));
    }

    let _ = relative;
    found
}

#[test]
fn dangerous_write_paths_are_whitelisted_with_a_reason() {
    let root = workspace_root();
    let mut per_file: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    let mut scanned = 0usize;

    for dir in SCAN_DIRS {
        let mut files = Vec::new();
        rs_files(&root.join(dir), &mut files);
        for file in files {
            let relative = file
                .strip_prefix(&root)
                .unwrap_or(&file)
                .to_string_lossy()
                .replace('\\', "/");
            let Ok(text) = fs::read_to_string(&file) else { continue };
            scanned += 1;
            for (rule, detail) in violations(&relative, &text) {
                per_file
                    .entry(relative.clone())
                    .or_default()
                    .entry(rule)
                    .or_default()
                    .push(detail);
            }
        }
    }

    // 底线：**扫到的东西太少 = 守卫坏了**（路径写错、目录改名都会让它空转着全绿）
    assert!(
        scanned >= 20,
        "只扫到 {scanned} 个源文件，守卫在空转（检查 SCAN_DIRS 与工作区根目录）"
    );

    // 白名单是棘轮：多一处要人来重新看一眼（补理由），少一处要说清白名单该删了
    let mut problems = Vec::new();
    for (file, rules) in &per_file {
        for (rule, hits) in rules {
            let allowed = ALLOWED
                .iter()
                .find(|entry| entry.file == file && entry.rule == rule);
            match allowed {
                Some(entry) if entry.count == hits.len() => {}
                Some(entry) => problems.push(format!(
                    "{file} 的 {rule} 有 {} 处，白名单写的 {} 处——{}\n    {}",
                    hits.len(),
                    entry.count,
                    entry.why,
                    hits.join("\n    ")
                )),
                None => problems.push(format!(
                    "{file} 出现未白名单的 {rule}：\n    {}\n  \
                     要么改成安全的写法，要么在 ALLOWED 里补一条**写清理由**的例外",
                    hits.join("\n    ")
                )),
            }
        }
    }
    for entry in ALLOWED {
        let hits = per_file
            .get(entry.file)
            .and_then(|rules| rules.get(entry.rule))
            .map(Vec::len)
            .unwrap_or(0);
        if hits == 0 {
            problems.push(format!(
                "白名单过期：{} 的 {} 已经一处都没有了（理由：{}）——把这条删掉",
                entry.file, entry.rule, entry.why
            ));
        }
    }

    assert!(
        problems.is_empty(),
        "危险写路径守卫没过：\n{}",
        problems.join("\n")
    );
}

// ─── 守卫自己的测试 ───────────────────────────────────────────────────────
//
// 扫描器写歪了比没有守卫更危险：它会**一直报绿**。所以下面把"认不认得出"逐条钉住。

fn rules_of(source: &str) -> Vec<String> {
    let mut found: Vec<String> = violations("test.rs", source)
        .into_iter()
        .map(|(rule, _)| rule)
        .collect();
    found.sort();
    found
}

#[test]
fn guard_detects_each_forbidden_shape() {
    // 删稿（带 WHERE 也是删稿：必须白名单）
    assert_eq!(
        rules_of(r#"conn.execute("DELETE FROM nodes WHERE id = ?1", [])"#),
        vec!["delete-precious"]
    );
    // 无 WHERE 的 DELETE：两条都要报（既没 WHERE、又多半是全表清）
    assert_eq!(
        rules_of(r#"conn.execute("DELETE FROM nodes", [])"#),
        vec!["delete-precious", "delete-without-where"]
    );
    // 正文表与作品表同样算稿件
    assert!(rules_of(r#""DELETE FROM node_contents WHERE node_id = 1""#).contains(&"delete-precious".into()));
    assert!(rules_of(r#""DELETE FROM works WHERE id = 1""#).contains(&"delete-precious".into()));
    // 不带 WHERE 的 UPDATE
    assert_eq!(
        rules_of(r#"conn.execute("UPDATE nodes SET title = ?1", [])"#),
        vec!["update-without-where"]
    );
    // DROP TABLE
    assert_eq!(rules_of(r#"conn.execute("DROP TABLE nodes", [])"#), vec!["drop-table"]);
    // 删目录
    assert_eq!(rules_of("let _ = std::fs::remove_dir_all(&dir);"), vec!["remove-dir-all"]);
}

#[test]
fn guard_allows_the_safe_shapes() {
    // 带 WHERE 的改写：正常写法，不报
    assert!(rules_of(r#""UPDATE nodes SET title = ?1 WHERE id = ?2""#).is_empty());
    // upsert：`ON CONFLICT ... DO UPDATE SET` 没有 WHERE，但它是"插入或更新一行"，不是全表改
    assert!(rules_of(
        r#""INSERT INTO writing_days (day, work_id) VALUES (?1, ?2) ON CONFLICT(day, work_id) DO UPDATE SET chars = ?3""#
    )
    .is_empty());
    // 索引表与设置表的删除（不在稿件表里，且带 WHERE）
    assert!(rules_of(r#""DELETE FROM settings WHERE key = ?1""#).is_empty());
    assert!(rules_of(r#""DELETE FROM node_fts WHERE rowid = old.id""#).is_empty());
    // 只删临时探测表
    assert!(rules_of(r#"conn.execute("DROP TABLE temp.__fts5_probe", [])"#).is_empty());
    // 注释里的示例不算数（否则文档一写就误报）
    assert!(rules_of("// 宁可写成 \"DELETE FROM nodes WHERE id = 1\" 也要带 WHERE").is_empty());
    // 触发器体里的多条语句按 `;` 切开逐条看：带 WHERE 就不报
    assert!(rules_of(
        r#""CREATE TRIGGER t AFTER UPDATE OF title ON nodes BEGIN
             DELETE FROM node_fts WHERE rowid = old.id;
             UPDATE node_fts SET title = new.title WHERE rowid = new.id;
           END""#
    )
    .is_empty());
}
