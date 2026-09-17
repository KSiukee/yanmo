# 第三方声明（THIRD_PARTY_NOTICES）

> 本文件从第一天维护。新增依赖、内置素材（字体 / 图标 / 词库 / 模型权重）时必须同步登记。

## 运行时依赖

| 组件 | 许可 | 用途 | 备注 |
|---|---|---|---|
| [Tauri](https://tauri.app/) | MIT 或 Apache-2.0 | 桌面壳（Rust 侧） | 二选一，本项目按 MIT 使用 |
| [serde](https://serde.rs/) / serde_json | MIT 或 Apache-2.0 | 序列化 | — |
| [rusqlite](https://github.com/rusqlite/rusqlite) / SQLite | MIT / 公有领域 | 本地数据库（WAL、全文检索） | `bundled` 特性：随二进制编译 SQLite 源码，版本可控 |
| [Vue 3](https://vuejs.org/) | MIT | 前端框架 | — |
| [Vite](https://vite.dev/) | MIT | 前端构建 | 开发期依赖 |
| [TypeScript](https://www.typescriptlang.org/) | Apache-2.0 | 前端类型检查 | 开发期依赖 |
| [vue-tsc](https://github.com/vuejs/language-tools) | MIT | 单文件组件（`.vue`）的类型检查 | 开发期依赖；随它进来的 @volar/typescript 与 @vue/language-core 也是 MIT |
| [TipTap](https://tiptap.dev/) / [ProseMirror](https://prosemirror.net/) | MIT | 正文编辑器内核 | — |

## 测试依赖（不随产品分发）

| 组件 | 许可 | 用途 |
|---|---|---|
| [tempfile](https://github.com/Stebalien/tempfile) | MIT 或 Apache-2.0 | 临时目录（数据库与数据目录相关测试） |

## 内置素材

| 素材 | 许可 | 状态 |
|---|---|---|
| 应用图标（墨点） | 本项目自制 | ✅ 无第三方权利 |
| 字体 | — | ⚠️ **当前不内置任何字体文件**，只用系统字体栈 |

### 字体纪律（重要）

中文字体大量为商业授权（方正、汉仪、微软雅黑等），**内置进安装包分发存在侵权风险**。
本项目规则：

1. **默认只用系统字体栈**；
2. 将来若内置，**仅限开源许可字体**（思源黑体 / 思源宋体 SIL OFL、霞鹜文楷 OFL、更纱黑体 OFL 等），并在此登记；
3. **用户自行导入的字体由用户自行承担其授权**（产品需在 UI 中说明）。

## 未来会引入（尚未引入）

| 组件 | 许可 | 注意 |
|---|---|---|
| SenseVoiceSmall 模型权重 | **FunASR Model Open Source License v1.1** | 可商用，但**必须署名并保留模型名**；权重不随仓库分发，由用户自行下载 |
| SenseVoiceSmall 代码 | MIT | — |
| 敏感词词库 | 视来源而定 | 引入前必须确认许可与合规性 |
