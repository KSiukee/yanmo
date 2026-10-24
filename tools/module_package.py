#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""模块平台随包：编宿主/启动器、放到 Tauri `externalBin` 位置、按发布配置打包。

从 `build_release.py` 拆出来的（那支脚本是发布流程的组合根，已经有 500 多行）。
这里的三个名字必须与 `app/build.rs` / `app/tauri.release.conf.json` 保持一致：
`yanmo-host`、`yanmo-launch`、`app/binaries/<name>-<三元组>.exe`。
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

HOST = "yanmo-host"
LAUNCH = "yanmo-launch"
STAGE_REL = Path("app/binaries")

for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        _stream.reconfigure(encoding="utf-8", errors="replace")


def _run(cmd: list, cwd: Path, timeout: int) -> tuple[int, str]:
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


def _host_triple(root: Path, cargo: str) -> str:
    if not cargo:
        return ""
    code, out = _run([cargo, "-vV"], root, 60)
    if code != 0:
        return ""
    match = re.search(r"^host:\s*(\S+)$", out, re.M)
    return match.group(1) if match else ""


def build_and_stage(root: Path, cargo: str) -> tuple[bool, str]:
    """编出 host/launch 并放到 `app/binaries/`；两个都成功才算成功。"""
    if not cargo:
        return False, "没找到 cargo"
    code, out = _run(
        [cargo, "build", "--release", "-p", "yanmo-host", "-p", "yanmo-launcher"],
        root,
        3600,
    )
    if code != 0:
        return False, out
    triple = _host_triple(root, cargo)
    if not triple:
        return False, "读不到主机三元组（cargo -vV 里没有 host: 那行）"
    stage_dir = root / STAGE_REL
    stage_dir.mkdir(parents=True, exist_ok=True)
    for name in (HOST, LAUNCH):
        source = root / "target" / "release" / f"{name}.exe"
        if not source.is_file():
            return False, f"编完了却找不到 {source}"
        shutil.copy2(source, stage_dir / f"{name}-{triple}.exe")
    return True, f"{HOST} / {LAUNCH}（{triple}）"


def bundle(tauri_cmd: list, no_bundle: bool, app: Path, runner) -> tuple[bool, str]:
    """用发布配置打包：`externalBin` 带上 host/launch。"""
    cmd = [*tauri_cmd, "build"]
    if no_bundle:
        cmd.append("--no-bundle")
    cmd.extend(["--config", "tauri.release.conf.json"])
    os.environ["YANMO_MODULE_PLATFORM"] = "1"
    return runner(cmd, cwd=app, timeout=7200)
