#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""研墨一键构建：**拿到仓库，一条命令得到一个能双击运行的安装包**。

用法（Windows）::

    tools\\build-release.bat                 # 自检 → 测试 → 打包 → 归集 + SHA256
    tools\\build-release.bat --no-bundle     # 只出可执行文件（不出安装包）
    tools\\build-release.bat --skip-tests    # 跳过测试（本地反复出包时用；正式出包别用）
    tools\\build-release.bat --out release      # 换个归集目录

# 为什么不能直接 `cargo build --release`

单独 `cargo build --release` 出来的是**开发模式**的可执行文件：它启动后会去连
`http://localhost:1420`（开发服务器），双击就是一片白屏。要出一个能直接双击的产物，
必须走 Tauri 自己的构建路径——`cargo tauri build` 会带上生产协议、把前端打进二进制，
再打成安装包。这条纪律**写进脚本**（脚本只走那条路），也写进 README。

# 六步，任一步失败就不出包

1. **工具链自检**：cargo / rustc / node / npm / tauri CLI 逐项检查，缺了给人话提示；
   WebView2 只提示不拦（它没装的话，安装包会带联网引导）。
2. **版本一致**：Cargo 工作区、应用配置、更新日志首条三处必须一致。
3. **前端依赖与构建**：`node_modules` 缺失才装；再跑一次生产构建。
4. **跑测试**：优先跑仓库自带的 QA 门（它更全）；没有就退到公开的两件
   （Rust 全仓测试 + 前端测试）。**失败就不出包**——宁可不发，也不发一个没测过的包。
5. **打包**：`cargo tauri build`（只出 NSIS 安装器）。
6. **归集与自检**：产物拷进 `dist/`，文件名带版本号，附 SHA256；结尾验证产物存在、不为空。

退出码：全过 0；任一步失败 1（并把失败那一步的输出尾巴打出来，不用去翻日志）。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "app"
FRONTEND = APP / "frontend"
DEFAULT_OUT = ROOT / "dist"
# 安装包小于这个大小就当作没打出来（空文件 / 半截文件都拦得住）
MIN_INSTALLER_BYTES = 1_000_000
MIN_BINARY_BYTES = 2_000_000


def make_output_readable() -> None:
    """让中文在 Windows 控制台里别变成乱码（切代码页 + 固定 UTF-8 输出）。"""
    if os.name == "nt":
        try:
            import ctypes

            ctypes.windll.kernel32.SetConsoleOutputCP(65001)
        except Exception:
            pass
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    except (AttributeError, OSError):
        pass


def run(cmd: list, cwd: Path | None = None, timeout: int = 3600) -> tuple[int, str]:
    env = dict(os.environ, PYTHONIOENCODING="utf-8")
    try:
        done = subprocess.run(
            [str(part) for part in cmd],
            cwd=str(cwd or ROOT),
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            env=env,
            timeout=timeout,
        )
        return done.returncode, (done.stdout or "") + (done.stderr or "")
    except FileNotFoundError as error:
        return 127, f"找不到可执行文件：{error}"
    except subprocess.TimeoutExpired:
        return 124, f"超时（超过 {timeout} 秒）"
    except OSError as error:
        return 126, f"跑不起来：{error}"


def tail(text: str, lines: int = 14) -> str:
    kept = [line for line in text.splitlines() if line.strip()]
    return "\n".join(kept[-lines:])


def find_tool(*names: str, env: str = "") -> str | None:
    if env and os.environ.get(env):
        return os.environ[env]
    for name in names:
        found = shutil.which(name)
        if found:
            return found
    return None


def say(ok: bool, name: str, detail: str = "") -> None:
    print(f"  {'✅' if ok else '❌'} {name:<12} {detail}".rstrip())


def fail(name: str, output: str) -> int:
    print(f"\n构建停下：{name}")
    for line in tail(output).splitlines():
        print(f"      │ {line}")
    print("\n（修好之后再跑同一条命令；没有产物就不会有半成品混进 dist/）")
    return 1


def project_version() -> tuple[str, list[str]]:
    """版本号：Cargo 工作区 / 应用配置 / 更新日志首条 / README 的「当前版本」——四处必须一致。

    为什么把 README 也拉进来：它写着"当前版本 X"，而那是最容易忘的一处——
    忘了改就会出现"README 说 0.41.8、包里是 0.42.0"这种对不上的事。
    靠检查单上写一条"记得改 README"是没用的，靠机器拦住才对。
    """
    found: dict[str, str] = {}
    cargo = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    section = re.search(r"\[workspace\.package\](.*?)(?:\n\[|\Z)", cargo, re.S)
    match = re.search(r'version\s*=\s*"([^"]+)"', section.group(1) if section else cargo)
    found["Cargo.toml"] = match.group(1) if match else ""
    try:
        found["tauri.conf.json"] = json.loads(
            (APP / "tauri.conf.json").read_text(encoding="utf-8")
        ).get("version", "")
    except (OSError, json.JSONDecodeError) as error:
        found["tauri.conf.json"] = f"<读不出来：{error}>"
    changelog = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    heading = re.search(r"^##\s+(\d+\.\d+\.\d+)", changelog, re.M)
    found["CHANGELOG.md"] = heading.group(1) if heading else ""
    readme = (ROOT / "README.md").read_text(encoding="utf-8")
    stated = re.search(r"当前版本\**\s*[:：]\s*\**(\d+\.\d+\.\d+)", readme)
    found["README.md"] = stated.group(1) if stated else ""

    problems = [f"{name}={value or '空'}" for name, value in found.items() if value != found["Cargo.toml"]]
    return found["Cargo.toml"], problems


def check_toolchain() -> tuple[bool, dict[str, str]]:
    """工具链自检：缺必需的给人话提示并停下；WebView2 只提示。"""
    tools: dict[str, str] = {}
    ok = True

    cargo = find_tool("cargo", "cargo.exe", env="CARGO")
    if cargo:
        code, out = run([cargo, "--version"], timeout=60)
        tools["cargo"] = out.strip() if code == 0 else ""
        say(code == 0, "cargo", tools["cargo"])
        ok = ok and code == 0
    else:
        say(False, "cargo", "没找到——先装 Rust 工具链（https://rustup.rs）")
        ok = False

    node = find_tool("node", "node.exe", env="NODE")
    npm = find_tool("npm", "npm.cmd", "npm.exe", env="NPM")
    if node and npm:
        code, out = run([node, "--version"], timeout=60)
        tools["node"] = out.strip() if code == 0 else ""
        say(code == 0 and bool(tools["node"]), "node", tools["node"])
        ok = ok and code == 0
        # tauri CLI：优先 `cargo tauri`（cargo 子命令），其次独立的 tauri。
        # 注意**命令**与**版本号文本**分开存：拿版本号当命令用是个真错误（自己踩过）。
        tauri_cmd: list | None = None
        if cargo:
            code, out = run([cargo, "tauri", "--version"], timeout=120)
            if code == 0:
                tauri_cmd = [cargo, "tauri"]
                tools["tauri"] = out.strip().splitlines()[-1] if out.strip() else "cargo tauri"
        if not tauri_cmd:
            standalone = find_tool("tauri", "tauri.cmd", "tauri.exe")
            if standalone:
                code, out = run([standalone, "--version"], timeout=120)
                if code == 0:
                    tauri_cmd = [standalone]
                    tools["tauri"] = out.strip() or standalone
        if tauri_cmd:
            tools["tauri_cmd"] = tauri_cmd
            say(True, "tauri", tools["tauri"])
        else:
            say(False, "tauri", "没找到——装一个：cargo install tauri-cli --version '^2'（或 npm i -g @tauri-apps/cli）")
            ok = False
    else:
        say(False, "node/npm", "没找到——先装 Node.js（https://nodejs.org）")
        ok = False

    # WebView2 只提示：没装的话安装包会带联网引导，不算构建失败
    webview = False
    for candidate in (
        os.environ.get("ProgramFiles(x86)", ""),
        os.environ.get("ProgramFiles", ""),
    ):
        if candidate and (Path(candidate) / "Microsoft/EdgeWebView/Application").is_dir():
            webview = True
    say(True, "WebView2", "本机有运行时" if webview else "本机没探到（安装包会带联网引导，不影响出包）")
    tools["webview2"] = "有运行时" if webview else "未探到"
    return ok, tools


def check_no_data_in_bundle() -> bool:
    """`data/` 绝不进安装包：安装包只装程序与资源，稿子在「文档」里。

    这里做**配置层**的机械断言（不设 resources / externalBin 指向数据目录），
    真正的装机验收（卸了之后稿子还在）在装机冒烟那一步做。
    """
    conf = json.loads((APP / "tauri.conf.json").read_text(encoding="utf-8"))
    bundle = conf.get("bundle", {})
    suspicious = []
    for key in ("resources", "externalBin", "files"):
        value = bundle.get(key)
        if not value:
            continue
        items = value if isinstance(value, list) else list(value.keys())
        suspicious += [str(item) for item in items if "data" in str(item).lower()]
    if suspicious:
        say(False, "data 不进包", f"应用配置里疑似把数据打进了安装包：{suspicious}")
        return False
    say(True, "data 不进包", "安装包只含程序与资源（稿子在「文档/研墨」，不进包）")
    return True


def ensure_frontend_deps(npm: str) -> tuple[bool, str]:
    if (FRONTEND / "node_modules").is_dir():
        return True, "已有 node_modules，跳过安装"
    code, out = run([npm, "install"], cwd=FRONTEND, timeout=1800)
    return code == 0, out


def build_frontend(npm: str) -> tuple[bool, str]:
    code, out = run([npm, "run", "build"], cwd=FRONTEND, timeout=1800)
    return code == 0, out


def run_tests(skip: bool) -> tuple[bool, str, str]:
    """测试：如果这个工作区里带了**额外的本地检查脚本**就先跑它（开发环境更全）；没有就退到公开两件。"""
    if skip:
        return True, "跳过（--skip-tests）", ""
    local_gate = ROOT / "tools" / "qa" / "check_all.py"
    cargo = find_tool("cargo", "cargo.exe", env="CARGO")
    if local_gate.is_file():
        code, out = run([sys.executable, str(local_gate), "--full"], timeout=7200)
        return code == 0, "本地加严检查（静态 + 逻辑 + 规模）", out
    # 公开路径：没有内部工具时也能自证
    code, out = run([cargo, "test", "--workspace"], timeout=3600)
    if code != 0:
        return False, "Rust 全仓测试", out
    node = find_tool("node", "node.exe", env="NODE")
    files = sorted(str(path) for path in (FRONTEND / "src").rglob("*.test.ts"))
    code, front = run([node, "--test", *files], cwd=FRONTEND, timeout=1800)
    if code != 0:
        return False, "前端测试", front
    return True, f"Rust 全仓 + 前端（{len(files)} 个测试文件）", ""


def bundle(tauri_cmd: list, no_bundle: bool) -> tuple[bool, str]:
    """**只走 Tauri 自己的构建路径**（生产协议 + 前端内嵌 + 打包）。"""
    cmd = [*tauri_cmd, "build"]
    if no_bundle:
        cmd.append("--no-bundle")
    code, out = run(cmd, cwd=APP, timeout=7200)
    return code == 0, out


def collect(out_dir: Path, version: str, no_bundle: bool) -> tuple[Path | None, str]:
    """把产物收进 `dist/`，名字带版本号。

    选包**按当前版本号精确匹配**，不是"字典序取最后一个"——后者在 `0.26.9` 之后出
    `0.26.10` 时会挑错包（字典序里 `9` 比 `1` 大）。这个坑是用户在真机上发现的：
    他手上那个安装包的属性写着旧版本号，一查是我们老早复制出去的那一份。
    """
    if no_bundle:
        built = ROOT / "target/release/yanmo.exe"
        wanted = f"研墨-{version}.exe"
    else:
        folder = ROOT / "target/release/bundle/nsis"
        candidates = sorted(folder.glob("*.exe")) if folder.is_dir() else []
        matched = [path for path in candidates if f"_{version}_" in path.name]
        if not matched:
            return None, (
                f"bundle 目录里没有 {version} 的安装包（现有："
                + ("、".join(path.name for path in candidates) or "空")
                + "）——版本号是不是没同步？"
            )
        built = matched[-1]
        wanted = f"研墨-{version}-setup.exe"
    if not built or not built.is_file():
        return None, f"没找到打包产物（找过 {built or 'bundle 目录不存在'}）"
    out_dir.mkdir(parents=True, exist_ok=True)
    # 顺手提醒：dist 里还躺着别的版本，别拿旧包去装（用户已经踩过一次）
    stale = sorted(
        path.name
        for path in out_dir.glob("研墨-*")
        if version not in path.name and path.suffix in {".exe", ".sha256"}
    )
    if stale:
        say(True, "旧包提醒", f"dist 里还有别的版本：{'、'.join(stale)}")
    target = out_dir / wanted
    shutil.copy2(built, target)
    digest = hashlib.sha256(target.read_bytes()).hexdigest()
    # UTF-8：产物文件名带中文（`研墨-<版本>-setup.exe`），按 ASCII 写会当场炸
    (out_dir / f"{wanted}.sha256").write_text(f"{digest}  {wanted}\n", encoding="utf-8")
    return target, digest


def source_commit() -> tuple[str, bool]:
    """当前源码提交与"工作区是不是脏的"——构建指纹的一半。

    为什么要记这个：只给产物哈希，别人没法判断"你这包是不是从这份源码出来的"；
    把提交一起写下来，核对才有起点（配合可复现构建，见 RELEASING.md）。
    """
    code, out = run(["git", "rev-parse", "HEAD"], timeout=60)
    commit = out.strip() if code == 0 else ""
    if not commit:
        return "", False
    code, out = run(["git", "status", "--porcelain"], timeout=60)
    dirty = code == 0 and bool(out.strip())
    return commit, dirty


def build_fingerprint(
    out_dir: Path,
    version: str,
    tools: dict[str, str],
    artifact: Path,
    digest: str,
) -> Path:
    """把"这一包到底是怎么来的"写成一页纸：版本 / 提交 / 工具链 / 产物指纹。

    诚实说明为什么不做"逐字节可复现"的宣称：Windows 安装包里含时间戳等非确定性内容，
    逐字节一致做不到；能给的、也确实有用的是**同一提交 + 同一工具链 → 功能等价的产物**，
    加上一个可核对的产物指纹。写在这里，比含糊说一句"可复现构建"更经得起追问。
    """
    commit, dirty = source_commit()
    stamp = time.strftime("%Y-%m-%d %H:%M:%S UTC", time.gmtime())
    lines = [
        "研墨 构建指纹",
        "=" * 48,
        f"版本：{version}",
        f"源码提交：{commit or '（不是 git 仓库 / 取不到）'}"
        + ("（**工作区有未提交改动**）" if dirty else ""),
        f"构建时间：{stamp}",
        f"平台：{platform.platform()}",
        "",
        "工具链：",
        f"  cargo / rustc：{tools.get('cargo') or '（未知）'}",
        f"  node：{tools.get('node') or '（未知）'}",
        f"  tauri：{tools.get('tauri') or '（未知）'}",
        f"  WebView2：{tools.get('webview2') or '（未探到）'}",
        "",
        f"产物：{artifact.name}（{artifact.stat().st_size} 字节）",
        f"SHA256：{digest}",
        "",
        "怎么用它核对（两步）：",
        f"  1) 核对产物没被掉包：certutil -hashfile {artifact.name} SHA256",
        "     —— 得到的哈希应与上面 SHA256 一行完全一致。",
        f"  2) 想自己构建：git checkout {commit[:12] if commit else '<提交>'}，用同一套工具链跑 "
        "tools\\build-release.bat",
        "",
        "诚实说明：Windows 安装包内含时间戳等非确定性内容，**逐字节一致做不到**；",
        "这里给的是「同一提交 + 同一工具链 → 功能等价的产物」以及可核对的产物指纹。",
        "",
    ]
    target = out_dir / f"构建指纹-{version}.txt"
    target.write_text("\n".join(lines), encoding="utf-8")
    return target


def main() -> int:
    make_output_readable()
    parser = argparse.ArgumentParser(description="研墨一键构建（自检 → 测试 → 打包 → 归集）")
    parser.add_argument("--no-bundle", action="store_true", help="只出可执行文件，不出安装包")
    parser.add_argument("--skip-tests", action="store_true", help="跳过测试（正式出包别用）")
    parser.add_argument("--out", default=str(DEFAULT_OUT), help="产物归集目录（默认 dist/）")
    args = parser.parse_args()

    print("研墨 · 一键构建")
    print("─" * 64)
    print("[1/6] 工具链自检")
    toolchain_ok, tools = check_toolchain()
    if not toolchain_ok:
        print("\n构建停下：工具链不全（上面的提示逐条照做即可）。")
        return 1

    print("[2/6] 版本一致")
    version, problems = project_version()
    if problems:
        say(False, "版本一致", "；".join(problems))
        return 1
    say(True, "版本一致", f"{version}（Cargo 工作区 / 应用配置 / 更新日志首条）")
    if not check_no_data_in_bundle():
        return 1

    print("[3/6] 前端依赖与构建")
    npm = find_tool("npm", "npm.cmd", "npm.exe", env="NPM")
    ok, out = ensure_frontend_deps(npm)
    say(ok, "前端依赖", out if ok else "装依赖失败")
    if not ok:
        return fail("前端依赖（npm install）", out)
    ok, out = build_frontend(npm)
    say(ok, "前端构建", "生产构建通过" if ok else "构建失败")
    if not ok:
        return fail("前端构建（npm run build）", out)

    print("[4/6] 跑测试")
    ok, label, out = run_tests(args.skip_tests)
    say(ok, "测试", label if ok else f"{label} 失败")
    if not ok:
        return fail(f"测试（{label}）", out)

    print("[5/6] 打包" + ("（不生成安装包）" if args.no_bundle else "（NSIS 安装器）"))
    ok, out = bundle(tools.get("tauri_cmd", []), args.no_bundle)
    if not ok:
        return fail("打包（cargo tauri build）", out)
    say(True, "打包", "完成")

    print("[6/6] 归集与自检")
    artifact, detail = collect(Path(args.out), version, args.no_bundle)
    if artifact is None:
        say(False, "归集", detail)
        return 1
    size = artifact.stat().st_size
    floor = MIN_BINARY_BYTES if args.no_bundle else MIN_INSTALLER_BYTES
    if size < floor:
        say(False, "自检", f"产物只有 {size} 字节，像是没打全")
        return 1
    say(True, "自检", f"产物 {size / 1024 / 1024:.1f} MB、校验文件已写")
    fingerprint = build_fingerprint(Path(args.out), version, tools, artifact, detail)
    say(True, "构建指纹", f"{fingerprint.name}（版本 / 提交 / 工具链 / 产物哈希）")
    print("─" * 64)
    print(f"产物：{artifact}")
    print(f"SHA256：{detail}")
    print(f"指纹：{fingerprint}")
    if args.no_bundle:
        print("下一步：双击这个可执行文件（它是生产模式，不会去连开发服务器）。")
    else:
        print("下一步：双击安装包；装完请按「装机冒烟」清单走一遍（装 / 首启 / 写 / 存 / 关 / 重开 / 卸载）。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
