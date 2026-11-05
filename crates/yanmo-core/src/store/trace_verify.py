#!/usr/bin/env python3
"""研墨 · 创作留痕包 独立校验（只依赖 Python 标准库，不需要装研墨）。

用法：python verify.py        （或双击，若系统关联了 .py）

它做三件事：
1. 按 manifest.json 里的 SHA-256 逐个核对文件；
2. 按固定字段顺序重算 events.jsonl 的链（seq / at / kind / subject / detail，
   紧凑 JSON、无空格、detail 内部保持原顺序）；
3. 核对条数与链头。

任何一步对不上就非零退出，并指出第几条 / 哪个文件。
"""
import hashlib
import json
import sys
from pathlib import Path

try:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
except Exception:
    pass

root = Path(__file__).resolve().parent


def fail(problems):
    print("✗ 校验不通过：")
    for item in problems:
        print("  -", item)
    sys.exit(1)


manifest_path = root / "manifest.json"
if not manifest_path.is_file():
    fail(["找不到 manifest.json（要把 verify.py 和包放在同一层）"])
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))

problems = []
for entry in manifest.get("files", []):
    path = root / entry["name"]
    if not path.is_file():
        problems.append("缺文件：" + entry["name"])
        continue
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != entry["sha256"]:
        problems.append("文件被改过：" + entry["name"])
    elif len(data) != entry["bytes"]:
        problems.append("文件大小对不上：" + entry["name"])

events_path = root / "events.jsonl"
if not events_path.is_file():
    fail(problems + ["找不到 events.jsonl"])

prev = "genesis"
count = 0
for index, line in enumerate(events_path.read_text(encoding="utf-8").splitlines()):
    if not line.strip():
        continue
    try:
        event = json.loads(line)
    except json.JSONDecodeError as error:
        problems.append(f"第 {index + 1} 行不是合法 JSON：{error}")
        break
    body = json.dumps(
        {
            "seq": event["seq"],
            "at": event["at"],
            "kind": event["kind"],
            "subject": event["subject"],
            "detail": event["detail"],
        },
        ensure_ascii=False,
        separators=(",", ":"),
    ).encode("utf-8")
    digest = hashlib.sha256(prev.encode("utf-8") + b"\n" + body).hexdigest()
    if event["seq"] != count + 1:
        problems.append(f"第 {index + 1} 条序号不连续（{event['seq']} != {count + 1}）")
    if event["prev"] != prev:
        problems.append(f"第 {index + 1} 条接到上一条的哈希不对（断链）")
    if digest != event["digest"]:
        problems.append(f"第 {index + 1} 条内容被改过（digest 对不上）")
    prev = digest
    count += 1

if count != manifest.get("event_count"):
    problems.append(f"条数对不上：文件里 {count} 条，manifest 写 {manifest.get('event_count')}")
if prev != manifest.get("chain_head"):
    problems.append("链头对不上：重算结果与 manifest 的 chain_head 不一致")

if problems:
    fail(problems)
print(f"✓ 校验通过：{count} 条事件，链头 {prev[:16]}…")
print("  注意：这只证明包内自洽与未被改动；不证明作者身份，也不证明本机时间没被改过。")
