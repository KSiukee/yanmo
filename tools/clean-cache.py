#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""清一清构建缓存：`target/` 会悄悄胖到几十 GB（实测攒到过 148 GB）。

用法::

    python3 tools/clean-cache.py                        # 只报数（默认什么也不删）
    python3 tools/clean-cache.py --clean                # 删最胖也最没用的那一块：debug/incremental
    python3 tools/clean-cache.py --clean --all-debug    # 连整个 debug 一起删（下次测试要全量重编）
    python3 tools/clean-cache.py --clean --release      # 连 release 一起删（那份能直接双击的 exe 也会没）

# 为什么单独有这么一件

`cargo` 只认**当前指纹**那一套产物：同一份代码在不同构建指纹下会各编一套（出包会往环境里塞
自己的变量，加上频繁的测试与检查），被取代的那几套一直躺在 `target/` 里不回收——于是它慢慢
胖成几十 GB（大头是 `debug/incremental` 与旧指纹的 `debug/deps`）。
而 `cargo clean` 一删就是整棵 `target/`，连你手上那份能直接双击的 exe 一起没。

这件工具只做一件事：**先说清哪块最胖，再按你说的删**。默认（不给 --clean）什么都不删。
仓库根的 `.cargo/config.toml` 已经把增量编译默认关掉，那是防它再长回来的第一道闸。

退出码：0 正常（默认只报数，什么也不删）。
血统：本项目自写（AGPL-3.0-or-later）。
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"

# 从最胖的开始（清的时候按这个顺序，先删最没用的）
BLOCKS = [
    ("debug/incremental", "增量编译的中间态：只影响\"改一点点重编\"快不快，删了最不心疼"),
    ("debug/deps", "debug 各指纹的产物与调试符号（.pdb 一份能到 150 MB）"),
    ("debug/build", "build 脚本产物"),
    ("debug", "整个 debug 树（测试与 check 用的那一套）"),
    ("release", "发行版产物——**那份能直接双击的 exe 在这里**，删了要重新出包"),
    ("doc", "cargo doc 的产物"),
]


def size_of(path: Path) -> tuple[int, int]:
    """(字节数, 文件数)；目录不在就当 0。"""
    if not path.exists():
        return 0, 0
    total = 0
    count = 0
    for base, _dirs, files in os.walk(path):
        for name in files:
            try:
                total += os.path.getsize(os.path.join(base, name))
                count += 1
            except OSError:
                continue  # 正被占用的文件跳过：报数不准一点，好过整件事跑不完
    return total, count


def human(num: int) -> str:
    for unit in ("B", "KB", "MB", "GB", "TB"):
        if num < 1024 or unit == "TB":
            return f"{num:.0f} {unit}" if unit in ("B", "KB") else f"{num:.2f} {unit}"
        num /= 1024
    return f"{num:.2f} TB"


def remove(path: Path) -> None:
    """删一棵树。Windows 上走原生 rmdir：几十万个碎文件时它比 shutil 快一个数量级。"""
    if not path.exists():
        return
    if os.name == "nt":
        subprocess.run(["cmd", "/c", "rmdir", "/s", "/q", str(path)], check=False)
        if path.exists():  # 原生那条没删干净（例如有文件被占用）时再兜一次
            shutil.rmtree(path, ignore_errors=True)
    else:
        shutil.rmtree(path, ignore_errors=True)


def report() -> None:
    total, count = size_of(TARGET)
    print(f"target/ 一共 {human(total)}、{count:,} 个文件")
    if not TARGET.is_dir():
        print("（还没有 target/——干净得很）")
        return
    print()
    print(f"{'块':<20}{'大小':>10}{'文件':>12}  说明")
    for name, note in BLOCKS:
        size, files = size_of(TARGET / name)
        if size == 0 and files == 0:
            continue
        print(f"{name:<20}{human(size):>10}{files:>12,}  {note}")


def main() -> int:
    parser = argparse.ArgumentParser(description="报出构建缓存哪块最胖，并按你说的清")
    parser.add_argument("--clean", action="store_true", help="真的删（默认只报数）")
    parser.add_argument("--incremental", action="store_true", help="删 debug/incremental（可与其他一起给）")
    parser.add_argument("--all-debug", action="store_true", help="删整个 debug/")
    parser.add_argument("--release", action="store_true", help="删 release/（那份能双击的 exe 也会没）")
    args = parser.parse_args()

    report()
    if not args.clean:
        print()
        print("只是报数（要真删就加 --clean，可配 --incremental / --all-debug / --release）")
        return 0

    # 不给具体目标时，默认只清最没用的那一块
    wanted: list[str] = []
    if args.all_debug:
        wanted.append("debug")
    elif args.incremental:
        wanted.append("debug/incremental")
    if args.release:
        wanted.append("release")
    if not wanted:
        wanted = ["debug/incremental"]

    print()
    freed = 0
    for name in wanted:
        path = TARGET / name
        size, _ = size_of(path)
        remove(path)
        freed += size
        state = "已删" if not path.exists() else "没删干净（有文件被占用？）"
        print(f"· {name} → {state}（{human(size)}）")
    print(f"合计释放约 {human(freed)}")
    report()
    return 0


if __name__ == "__main__":
    sys.exit(main())
