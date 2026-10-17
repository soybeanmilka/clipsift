# 手动发版脚本（CI 不可用时的备份方案）
#
# 用途：本地构建 Windows 版 clipsift.exe，打包成 zip，上传到 GitHub Release。
# CI（.github/workflows/release.yml）能跑时优先用 CI，这个脚本是兜底。
#
# 用法：
#   pwsh -File scripts/release.ps1 -Version 0.1.0
#
# 前置：已安装 Rust、GitHub CLI 且 `gh auth status` 正常。

param(
    [Parameter(Mandatory = $true)]
    [string]$Version
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

if ($Version -notmatch '^\d+\.\d+\.\d+$') {
    throw "版本号格式不对，应该是 0.1.0 这样：$Version"
}
$tag = "v$Version"

Write-Host "==> 1/5 跑测试" -ForegroundColor Cyan
cargo test --release
if ($LASTEXITCODE -ne 0) { throw "测试没通过，不发了" }

Write-Host "==> 2/5 构建 release" -ForegroundColor Cyan
cargo build --release --bin clipsift
if ($LASTEXITCODE -ne 0) { throw "构建失败" }

Write-Host "==> 3/5 打包" -ForegroundColor Cyan
$dist = Join-Path $root "dist"
if (Test-Path $dist) { Remove-Item $dist -Recurse -Force }
New-Item -ItemType Directory -Path $dist | Out-Null
Copy-Item "target\release\clipsift.exe" $dist
Copy-Item "README.md", "LICENSE" $dist
$zip = Join-Path $root "clipsift-windows-x86_64.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path "$dist\*" -DestinationPath $zip
$size = (Get-Item $zip).Length
Write-Host "    产物：$zip（$([math]::Round($size/1MB, 2)) MB）"

Write-Host "==> 4/5 打 tag 并推送" -ForegroundColor Cyan
git tag -a $tag -m "ClipSift $Version"
git push origin $tag

Write-Host "==> 5/5 创建 Release" -ForegroundColor Cyan
gh release create $tag $zip `
    --title "ClipSift $Version" `
    --notes "见 README。Windows x64 单文件，解压即用。" `
    --verify-tag

Write-Host "`n完成：https://github.com/soybeanmilka/clipsift/releases/tag/$tag" -ForegroundColor Green
