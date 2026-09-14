#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""从 CHANGELOG 里取"最新一节"当发布说明，并附上两段每次都一样的通用说明。

用法::

    python tools/release_notes.py            # 打到标准输出
    python tools/release_notes.py out.md     # 写进文件（CI 里这样用）

为什么单独一个脚本：发布说明的三半来源不同——"这一版做了什么"来自 CHANGELOG（人工写的），
"下载哪个 / 怎么装 / 第一次打开"与"拿到文件怎么核对"是每次都一样的两段（机器生成的）。
混在一起手抄，迟早有一版忘了写校验和。

**为什么把「上手三步」也搬进来**：多数人是直接落在 Release 页面上的——仓库首页那段
（README 里的「上手三步」）他们不一定翻。三个产物长得很像，页面上不说清哪个是哪个，
下载页就等于让人猜。
"""

from __future__ import annotations

import re
import sys
from pathlib import Path


# 控制台编码**不由我们决定**：GitHub 的 Windows 跑手默认代码页是 cp1252，
# 直接打印中文会 UnicodeEncodeError（真踩过：出包六步全绿，卡在最后那句「已写入…」，
# 于是 Release 一步都没走到）。这里把标准输出/错误统一切到 UTF-8，
# 编不出的字符退成 `?`——**打印失败绝不该让整条流水线挂掉**。
for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        _stream.reconfigure(encoding="utf-8", errors="replace")

ROOT = Path(__file__).resolve().parents[1]
CHANGELOG = ROOT / "CHANGELOG.md"

# 每次都要带的一段：**下载哪个 / 怎么装 / 第一次打开**（README「上手三步」的同一份内容，
# 版本号按本次发布填）。三个产物是一份源码、一次构建出来的——只是形态不同。
HOWTO = """
## 上手三步

### 1. 下载哪个（三个文件是一份源码、一次构建出来的，只是形态不同）

- **`yanmo-{version}-setup.exe`** —— **安装版**：开始菜单里能找到，随时可卸载。多数人选这个。
- **`yanmo-{version}-portable.exe`** —— **便携版**：双击即用，不写注册表，稿子仍放在系统里的数据位置。
- **`yanmo-{version}-portable.zip`** —— **绿色版**（解压即用）：稿子放在解压出来的那个文件夹里，
  跟着文件夹走（拷到 U 盘、拷到别的机器，稿子一起走）。适合"整个装在一个文件夹里带走"。

### 2. 怎么装

安装版双击跟向导走（按当前用户安装，不需要管理员权限，不加开机自启）；
便携版双击即用；绿色版解压到你想放的地方，双击里面的「研墨.exe」。

### 3. 第一次打开

它会先问你一句「稿子放哪」（给一个推荐位置，也可以换成别的盘）。进去以后正文上方有一条提示，
给两个入口：「**建一本书**」（一次问清书名、类型、简介、卷章命名、一卷大概多少章），
或者「**就在这一本里写**」（自动建的那本空白书还没名字，起个名就能开写）。之后就只剩写作本身了。
"""

# 每次都要带的一段：**怎么确认拿到的是原件**。
# 零出网、接口清单那些属于 SECURITY.md，这里只放"下载当天用得上"的两条，避免两处各写一遍。
FOOTER = """
---

## 拿到文件后怎么核对（两条）

1. **对校验和**：每个产物旁边都有一个同名 `.sha256` 文件，里面是它的 SHA256：

   ```bat
   certutil -hashfile <产物文件名> SHA256
   ```

   算出来的哈希应与 `.sha256` 文件里那一行完全一致；`yanmo-<版本>-build-fingerprint.txt` 里也记着同一个值，
   另外还写了这一包是哪次提交、用哪套工具链构建的。

2. **没签名，第一次运行会被 Windows 拦一下**：点「更多信息」→「仍要运行」即可。
   为什么暂时不签名、以及"零出网"怎么自己验证（看依赖树 / 跑检查 / 断网实测），都写在
   [SECURITY.md](https://github.com/KSiukee/yanmo/blob/main/SECURITY.md) 里，不在这儿重复一遍。

> 想自己从源码构建、出一份你自己的安装包：见
> [RELEASING.md](https://github.com/KSiukee/yanmo/blob/main/RELEASING.md)，
> 一条命令 `tools\\build-release.bat` 即可。
"""


def latest_section(text: str) -> str:
    """取第一条 `## ` 到第二条 `## ` 之间的内容（即最新版本那一节）。"""
    lines = text.splitlines()
    start = next((index for index, line in enumerate(lines) if line.startswith("## ")), None)
    if start is None:
        return ""
    end = next(
        (index for index in range(start + 1, len(lines)) if lines[index].startswith("## ")),
        len(lines),
    )
    return "\n".join(lines[start:end]).strip()


def version_of(section: str) -> str:
    """从 `## 0.49.0 — 2026-09-14` 里取版本号（产物文件名要用它）。

    认不出来就退回一个占位符——**宁可页面上写着 `<版本>`，也不要在这里报错**：
    发布说明是最后一步，挂在这儿等于前面的包白打。
    """
    first = section.splitlines()[0] if section else ""
    found = re.match(r"##\s+([0-9]+\.[0-9]+\.[0-9]+)", first)
    return found.group(1) if found else "<版本>"


def main() -> int:
    if not CHANGELOG.is_file():
        print("找不到 CHANGELOG.md", file=sys.stderr)
        return 1
    section = latest_section(CHANGELOG.read_text(encoding="utf-8"))
    if not section:
        print("CHANGELOG 里没有版本小节（应当以 `## 版本号 — 日期` 开头）", file=sys.stderr)
        return 1
    notes = section + "\n" + HOWTO.format(version=version_of(section)) + FOOTER
    if len(sys.argv) > 1:
        Path(sys.argv[1]).write_text(notes, encoding="utf-8")
        print(f"已写入 {sys.argv[1]}（{len(notes)} 字符）")
    else:
        print(notes)
    return 0


if __name__ == "__main__":
    sys.exit(main())
