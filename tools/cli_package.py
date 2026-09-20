#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""命令行工具随包：把它编出来，并放到 Tauri 的 `externalBin` 认的位置。

从 `build_release.py` 拆出来的（那支脚本是发布流程的组合根，长了就该把本体搬走；
同一条规矩也拆出过 `green_package.py`）。

# 为什么它必须随包（2026-09-20 立）

界面打不开（显卡 / 驱动 / 白屏）或者库坏到研墨起不来时，`yanmo-cli` 是**唯一**的取稿路，
而 README 与 SECURITY 有 7 处写着"随发行包附带"。以前出包脚本只编主程序，于是
`target/release/yanmo-cli.exe` 一直停在旧版本、发行包里根本没有它——**文档承诺与实际
交付对不上**。这个模块就是那件事的补丁。

放行它进安装包的那条**受控例外**写在 `app/tests/release_boundary.rs` 的守卫里：
`bundle.externalBin` 只许指 `binaries/yanmo-cli*`，`resources` / `files` 仍然一律禁止。

跑法（通常由 `tools/build-release.bat` 顺带调用）::

    python tools/cli_package.py --root . --cargo cargo
"""

from __future__ import annotations

import argparse
import re
import shutil
import subprocess
import sys
from pathlib import Path

# `tauri.conf.json` 的 `bundle.externalBin` 里写的名字
CLI_NAME = "yanmo-cli"
# 编出来的二进制（工作区的 release 产物）
CLI_REL = Path("target/release/yanmo-cli.exe")
# Tauri 取它的位置：`app/binaries/`，文件名必须带主机三元组
STAGE_REL = Path("app/binaries")


def _run(cmd: list, cwd: Path, timeout: int) -> tuple[int, str]:
    """跑一条命令，拿 `(退出码, 输出)`。

    与 `build_release.run` 同形，但**故意不互相 import**：那边是组合根，这边要能单独跑
    （`python tools/cli_package.py`），两个模块谁也不该依赖谁。
    """
    try:
        done = subprocess.run(
            [str(part) for part in cmd],
            cwd=str(cwd),
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=timeout,
        )
        return done.returncode, (done.stdout or "") + (done.stderr or "")
    except FileNotFoundError as error:
        return 127, f"找不到可执行文件：{error}"
    except subprocess.TimeoutExpired:
        return 124, f"超时（超过 {timeout} 秒）"
    except OSError as error:
        return 126, f"跑不起来：{error}"


def host_triple(root: Path, cargo: str) -> str:
    """主机三元组（如 `x86_64-pc-windows-msvc`）——`externalBin` 的文件名必须带它。

    从 `cargo -vV` 里读 `host:` 那行：既不额外要求 `rustc` 在 PATH 上，也不写死架构
    （写死的话，换一台 ARM 机器出包会突然"找不到文件"）。
    """
    if not cargo:
        return ""
    code, out = _run([cargo, "-vV"], root, 60)
    if code != 0:
        return ""
    match = re.search(r"^host:\s*(\S+)$", out, re.M)
    return match.group(1) if match else ""


def build(root: Path, cargo: str) -> tuple[bool, str]:
    """编出命令行工具；返回 `(成功?, 一句话)`。"""
    if not cargo:
        return False, "没找到 cargo"
    code, out = _run([cargo, "build", "--release", "-p", "yanmo-cli"], root, 3600)
    if code != 0:
        return False, out
    binary = root / CLI_REL
    if not binary.is_file():
        return False, f"编完了却找不到 {binary}"
    return True, f"{binary.stat().st_size / 1024 / 1024:.1f} MB"


def stage(root: Path, cargo: str) -> tuple[bool, str]:
    """把 CLI 放到 Tauri 的 `externalBin` 认的位置。

    这一步就是"安装包里也有命令行工具"的全部机关：`tauri.conf.json` 里只写
    `binaries/yanmo-cli`，而真正的文件必须在**打包之前**带三元组后缀就位。

    ⚠️ **它必须发生在跑测试之前**：`app` 的构建脚本（`app/build.rs` 之后是 Tauri 自己的检查）
    在**编译期**就要求这个资源存在，晚了连测试都编不过。0.72.4 就是这么撞出来的。
    """
    triple = host_triple(root, cargo)
    if not triple:
        return False, "读不到主机三元组（cargo -vV 里没有 host: 那行）"
    binary = root / CLI_REL
    if not binary.is_file():
        return False, f"还没有 CLI 可放：{binary}"
    staged_dir = root / STAGE_REL
    staged_dir.mkdir(parents=True, exist_ok=True)
    staged = staged_dir / f"{CLI_NAME}-{triple}.exe"
    shutil.copy2(binary, staged)
    return True, f"{staged.relative_to(root)}（{triple}）"


def build_and_stage(root: Path, cargo: str) -> tuple[bool, str]:
    """编出来 + 放到位；两步都成功才算成功。返回 `(成功?, 一句话)`。"""
    ok, detail = build(root, cargo)
    if not ok:
        return False, detail
    return stage(root, cargo)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="命令行工具随包（编出来 + 放到 externalBin 位置）")
    parser.add_argument("--root", default=".", help="仓库根（默认当前目录）")
    parser.add_argument("--cargo", default="cargo", help="cargo 可执行文件")
    args = parser.parse_args(argv)

    ok, detail = build_and_stage(Path(args.root).resolve(), args.cargo)
    print(("✅ " if ok else "❌ ") + detail)
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
