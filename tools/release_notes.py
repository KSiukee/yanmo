#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""从 CHANGELOG 里取"最新一节"当发布说明，并附上通用的验证指引。

用法::

    python tools/release_notes.py            # 打到标准输出
    python tools/release_notes.py out.md     # 写进文件（CI 里这样用）

为什么单独一个脚本：发布说明的两半来源不同——"这一版做了什么"来自 CHANGELOG（人工写的），
"拿到文件怎么核对"是每次都一样的一段（机器生成的）。混在一起手抄，迟早有一版忘了写校验和。
"""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHANGELOG = ROOT / "CHANGELOG.md"

# 每次都要带的一段：**怎么确认拿到的是原件**。
# 零出网、接口清单那些属于 SECURITY.md，这里只放"下载当天用得上"的两条，避免两处各写一遍。
FOOTER = """
---

## 拿到文件后怎么核对（两条）

1. **对校验和**：每个产物旁边都有一个同名 `.sha256` 文件，里面是它的 SHA256：

   ```bat
   certutil -hashfile <产物文件名> SHA256
   ```

   算出来的哈希应与 `.sha256` 文件里那一行完全一致；`构建指纹-<版本>.txt` 里也记着同一个值，
   另外还写了这一包是哪次提交、用哪套工具链构建的。

2. **没签名，第一次运行会被 Windows 拦一下**：点「更多信息」→「仍要运行」即可。
   为什么暂时不签名、以及"零出网"怎么自己验证（看依赖树 / 跑检查 / 断网实测），都写在
   [SECURITY.md](https://github.com/KbyAndroid/yanmo/blob/main/SECURITY.md) 里，不在这儿重复一遍。

> 想自己从源码构建、出一份你自己的安装包：见
> [RELEASING.md](https://github.com/KbyAndroid/yanmo/blob/main/RELEASING.md)，
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


def main() -> int:
    if not CHANGELOG.is_file():
        print("找不到 CHANGELOG.md", file=sys.stderr)
        return 1
    section = latest_section(CHANGELOG.read_text(encoding="utf-8"))
    if not section:
        print("CHANGELOG 里没有版本小节（应当以 `## 版本号 — 日期` 开头）", file=sys.stderr)
        return 1
    notes = section + "\n" + FOOTER
    if len(sys.argv) > 1:
        Path(sys.argv[1]).write_text(notes, encoding="utf-8")
        print(f"已写入 {sys.argv[1]}（{len(notes)} 字符）")
    else:
        print(notes)
    return 0


if __name__ == "__main__":
    sys.exit(main())
