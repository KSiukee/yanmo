# 随包示例

`bigmap-lite/` 是随安装包附带的示例模块（体验版大地图）：一个模块包该长什么样、
能声明哪些只读能力，看它就行。

- **它是什么**：读作品信息 / 目录 / 设定卡 / 伏笔，画成「地点 + 人物 + 还没收的线头」，
  再加一个章节滑块看「到这一章为止」的世界；**不读正文**（按词表找候选是完整版的能力）。
- **怎么装**：研墨 → 模块页 → 示例卡上点「装上看看」；走的是完整导入通道
  （预览 → 逐项勾权限 → 确认），跟从文件夹导入没有任何区别。
- **怎么验**：`cargo test -p yanmo-app --test sample_module`（守卫：程序在、界面在、
  源码在仓里、声明了只读能力、且**没有**声明读正文）。

## 源码（写模块的参考）

`bigmap-lite-src/` 是它的源码，**约 250 行、不依赖核心**，把「一个模块怎么读数据、
怎么把自己的结果存起来」这条件讲清楚：

```powershell
cd bigmap-lite-src
pwsh -NoProfile -File build.ps1   # 编 wasm 并覆盖 ../bigmap-lite/module.wasm
```

需要 `rustup target add wasm32-unknown-unknown`。源码与核心同为 **AGPL-3.0-or-later**。
