# 编出随包示例模块，并把产物放回包目录（../bigmap-lite/module.wasm）
#
#   pwsh -NoProfile -File build.ps1
#
# 需要：rustup target add wasm32-unknown-unknown
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

cargo build --release --target wasm32-unknown-unknown
if ($LASTEXITCODE -ne 0) { throw "cargo build 失败" }

$built = Join-Path $PSScriptRoot "target\wasm32-unknown-unknown\release\bigmap_lite_sample.wasm"
$target = Join-Path $PSScriptRoot "..\bigmap-lite\module.wasm"
Copy-Item $built $target -Force
Write-Output ("已更新：" + (Resolve-Path $target))
Get-Item $target | Select-Object Name, Length | Format-Table -AutoSize
