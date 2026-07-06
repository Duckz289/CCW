$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$packageJsonPath = Join-Path $repoRoot "package.json"
$packageInfo = Get-Content -Raw -LiteralPath $packageJsonPath | ConvertFrom-Json
$version = $packageInfo.version

$releaseExe = Join-Path $repoRoot "src-tauri\target\release\claude-cache-warden.exe"
$distRoot = Join-Path $repoRoot "dist-portable"
$portableDir = Join-Path $distRoot "ClaudeCacheWarden-portable"
$portableExe = Join-Path $portableDir "Claude Cache Warden (Portable).exe"
$portableReadme = Join-Path $portableDir "README-portable.txt"
$zipPath = Join-Path $distRoot "ClaudeCacheWarden-portable-v$version.zip"

function Assert-PathInsideRepo {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    $fullPath = [System.IO.Path]::GetFullPath($Path)
    $fullRoot = [System.IO.Path]::GetFullPath($repoRoot)
    if (-not $fullPath.StartsWith($fullRoot + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to write outside repository: $fullPath"
    }
}

Set-Location -LiteralPath $repoRoot

if (-not (Test-Path -LiteralPath $releaseExe)) {
    Write-Host "Release executable not found. Running npm run tauri:build..."
    $npm = Get-Command npm.cmd -ErrorAction SilentlyContinue
    if (-not $npm) {
        $npm = Get-Command npm -ErrorAction Stop
    }
    & $npm.Source run tauri:build
}

if (-not (Test-Path -LiteralPath $releaseExe)) {
    throw "Release executable still not found after build: $releaseExe"
}

Assert-PathInsideRepo -Path $distRoot
Assert-PathInsideRepo -Path $portableDir
Assert-PathInsideRepo -Path $zipPath

New-Item -ItemType Directory -Force -Path $distRoot | Out-Null
if (Test-Path -LiteralPath $portableDir) {
    Remove-Item -LiteralPath $portableDir -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $portableDir | Out-Null

Copy-Item -LiteralPath $releaseExe -Destination $portableExe -Force

$readmeText = @"
Claude Cache Warden Portable
============================

English
-------
This is the portable build. Double-click "Claude Cache Warden (Portable).exe" to run it directly. No installer is required.

If the app does not open, or Windows reports an error related to WebView2, install Microsoft Edge WebView2 Runtime from the official Microsoft page:
https://developer.microsoft.com/microsoft-edge/webview2/

This build is not code-signed yet. Windows SmartScreen may show "Windows protected your PC" on first launch. Click "More info", then "Run anyway" to continue.

Tiếng Việt
----------
Đây là bản portable. Bấm đúp vào "Claude Cache Warden (Portable).exe" để chạy trực tiếp, không cần cài đặt.

Nếu app không mở được, hoặc Windows báo lỗi liên quan đến WebView2, hãy cài Microsoft Edge WebView2 Runtime từ trang chính thức của Microsoft:
https://developer.microsoft.com/microsoft-edge/webview2/

Bản này chưa ký số. Windows SmartScreen có thể hiện cảnh báo "Windows protected your PC" trong lần chạy đầu tiên. Bấm "More info", rồi chọn "Run anyway" để tiếp tục.
"@

Set-Content -LiteralPath $portableReadme -Value $readmeText -Encoding UTF8

if (Test-Path -LiteralPath $zipPath) {
    Remove-Item -LiteralPath $zipPath -Force
}
Compress-Archive -LiteralPath $portableDir -DestinationPath $zipPath -CompressionLevel Optimal

$zip = Get-Item -LiteralPath $zipPath
[pscustomobject]@{
    PortableDirectory = $portableDir
    ZipPath = $zip.FullName
    ZipBytes = $zip.Length
    ZipMB = [math]::Round($zip.Length / 1MB, 2)
}
