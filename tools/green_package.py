#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""绿色版打包：把便携 exe 压成一个"解压即用、稿子跟着走"的 zip。

从 `build_release.py` 拆出来的（那支脚本是发布流程的组合根，长了就该把本体搬走）。

# 三个"想清楚了才敢这么写"的地方

1. **文件夹名不带版本号**：带版本号的话，"下载新版解压"会解出一个新文件夹，而稿子在
   旧文件夹的 `data\\` 里——作者看到的是一套空库（"稿子没了"的经典现场）。
   名字稳定，解压到同一个位置＝原地覆盖 exe＝真更新。
2. **必须带 `yanmo-portable.txt`**：核心只认这个标记（`paths::is_portable`），而它的
   注释写明"标记只能由**发布形态**带进来"。绿色包不带标记，就等于"免安装但稿子仍在
   系统位置"——"拔盘即走"是空话。
3. **许可证随包**：AGPL 分发二进制要随包给（与安装包同一条合规要求）。
4. **命令行工具也要在**（2026-09-20 补）：界面打不开时它是唯一的取稿路，README 与
   SECURITY 都写着"随包附带"——所以它**必须**进来，缺了宁可不出包（不是可选装饰）。

跑法（通常由 `tools/build-release.bat` 顺带调用）::

    python tools/green_package.py --out dist --version 0.45.0 \\
        --exe target/release/yanmo.exe --cli target/release/yanmo-cli.exe
"""

from __future__ import annotations

import argparse
import hashlib
import sys
import zipfile
from pathlib import Path

# 绿色包里的东西。文件夹名**故意不带版本号**（见模块头注释第 1 条）。
GREEN_FOLDER = "研墨"
PORTABLE_MARKER = "yanmo-portable.txt"
GREEN_README = "一页怎么用.txt"
# 命令行工具在包内的名字：与安装包装出来的那个保持一致（README / SECURITY 都按这个名写）。
CLI_NAME = "yanmo-cli.exe"

MARKER_NOTE = (
    "这个文件是给研墨看的：它表示\"绿色版\"——稿子放在本文件夹的 data\\ 里，\n"
    "跟着这个文件夹走（拷到别的盘、别的机器，稿子一起走）。\n"
    "删掉它，研墨就会改回把稿子放进系统里的默认位置（一般用不着删）。\n"
)


def green_readme(version: str) -> str:
    """绿色包里的「一页怎么用」——**只讲作者真要做的几件事**，不夹技术话。"""
    return f"""研墨 {version} · 绿色版（解压即用，不用安装）

一、怎么开始
   双击「研墨.exe」。第一次打开会问一句"稿子放哪"，选「就用这里」就行——
   稿子会放在本文件夹的 data\\ 里，跟着这个文件夹走。

二、我的稿子在哪
   就在本文件夹的 data\\ 里（一个数据库文件）。
   这个文件夹拷到别的盘、别的机器，稿子一起走。

三、怎么更新（重要）
   把新版压缩包**解压到同一个文件夹**（也就是覆盖这里的「研墨.exe」），
   千万**不要动 data\\ 这个文件夹**。打开就是新版，稿子还在。
   ⚠️ 别把新版解压到一个新文件夹：那里的 data\\ 是空的，看起来会像"稿子没了"
      （稿子没丢，还在旧文件夹里，但要你自己搬过来）。

四、想备份、想换电脑
   · 备份：研墨里有「备份」，也可以从「导出」把整本书导成文件。**别只备份在本文件夹**——
     这个文件夹（U 盘）丢了，稿子就一起丢了。
   · 换电脑：把整个文件夹拷过去即可。

五、界面打不开的时候（命令行工具）
   本文件夹里还有一个 `yanmo-cli.exe`：**双击它就是中文菜单**，不用记参数。
   界面起不来（显卡 / 驱动 / 白屏）、或者库坏到研墨打不开时，用它：
   · 看稿：列出书架、目录、读某一章；
   · 导出：把整本书导成 txt / json（换台机器也能接着写）；
   · 体检：告诉你库坏在哪一层；
   · 最后的退路：库里什么都没有、只剩一份备份包时，用它的「从成稿导入」
     （默认只看不写，确认无误再加 --yes；只新建，不动你原有的书）。
   ⚠️ 它也是**只读优先**的：菜单里那些"看"的动作不会改你的稿子。

六、它需要什么
   Windows 10/11。界面用的是系统自带的 WebView2：
   Win11 一定有；Win10 1803 以后的机器多半也自带。老机器或精简系统如果打开是白屏，
   请改用安装版（yanmo-{version}-setup.exe）。

七、许可证
   AGPL-3.0-or-later，全文见同目录的 LICENSE。
"""


def green_zip(
    out_dir: Path, version: str, exe: Path, license_file: Path, cli: Path
) -> tuple[Path | None, str]:
    """压出绿色包；返回 `(zip 路径, sha256)`，任一前提不满足就返回 `(None, 为什么)`。

    `cli` 是**必需**的（不是可选装饰）：界面打不开时的取稿路就在它身上，缺了宁可不出包。
    """
    if not exe.is_file():
        return None, f"没有便携 exe 可打包（找过 {exe}）"
    if not license_file.is_file():
        return None, f"找不到许可证：{license_file}（AGPL 二进制要随包给）"
    if not cli.is_file():
        return None, f"找不到命令行工具：{cli}（界面打不开时的取稿路，不能缺）"
    out_dir.mkdir(parents=True, exist_ok=True)
    wanted = out_dir / f"yanmo-{version}-portable.zip"
    with zipfile.ZipFile(wanted, "w", zipfile.ZIP_DEFLATED) as bundle:
        bundle.write(exe, f"{GREEN_FOLDER}/研墨.exe")
        bundle.write(cli, f"{GREEN_FOLDER}/{CLI_NAME}")
        bundle.writestr(f"{GREEN_FOLDER}/{PORTABLE_MARKER}", MARKER_NOTE)
        bundle.write(license_file, f"{GREEN_FOLDER}/LICENSE")
        bundle.writestr(f"{GREEN_FOLDER}/{GREEN_README}", green_readme(version))
    digest = hashlib.sha256(wanted.read_bytes()).hexdigest()
    # UTF-8：产物文件名带中文，按 ASCII 写会当场炸
    (out_dir / f"{wanted.name}.sha256").write_text(f"{digest}  {wanted.name}\n", encoding="utf-8")
    return wanted, digest


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="研墨绿色版打包（exe + 命令行工具 + 便携标记 + LICENSE + 一页怎么用）"
    )
    parser.add_argument("--out", required=True, help="产物归集目录（dist/）")
    parser.add_argument("--version", required=True, help="版本号（写进文件名与一页说明）")
    parser.add_argument("--exe", required=True, help="便携 exe 路径")
    parser.add_argument("--cli", required=True, help="命令行工具路径（yanmo-cli.exe）")
    parser.add_argument("--license", default="LICENSE", help="许可证路径（默认仓库根 LICENSE）")
    args = parser.parse_args(argv)

    zipped, detail = green_zip(
        Path(args.out), args.version, Path(args.exe), Path(args.license), Path(args.cli)
    )
    if zipped is None:
        print(f"绿色版打包失败：{detail}", file=sys.stderr)
        return 1
    print(f"绿色版：{zipped}")
    print(f"SHA256：{detail}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
