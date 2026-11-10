param(
  [string]$TargetDir = (Join-Path $PSScriptRoot '..\target'),
  [string]$OutputDir = (Join-Path $PSScriptRoot '..\dist'),
  [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$version = (Get-Content (Join-Path $root 'package.json') -Raw | ConvertFrom-Json).version
$displayVersion = $version -replace '\.0$', ''
$name = "OpenSchedule-v$displayVersion-win32-x64"
$output = [System.IO.Path]::GetFullPath($OutputDir)
$target = [System.IO.Path]::GetFullPath($TargetDir)
$package = Join-Path $output $name
$archive = Join-Path $output "$name.zip"
if ((Test-Path $package) -or (Test-Path $archive)) {
  throw "배포 대상이 이미 존재합니다: $package 또는 $archive"
}
if (-not $SkipBuild) {
  & cargo build --release --workspace --target-dir $target
  if ($LASTEXITCODE -ne 0) { throw 'Rust release 빌드에 실패했습니다.' }
}
$electronDist = Join-Path $root 'node_modules\electron\dist'
$icons = Join-Path $root 'node_modules\@phosphor-icons\web'
foreach ($required in @((Join-Path $electronDist 'electron.exe'), $icons,
    (Join-Path $target 'release\openlab.exe'), (Join-Path $target 'release\openschedule-agent.exe'),
    (Join-Path $target 'release\openschedule.exe'))) {
  if (-not (Test-Path $required)) { throw "배포 파일을 찾을 수 없습니다: $required" }
}
New-Item -ItemType Directory -Path $package -Force | Out-Null
Copy-Item (Join-Path $electronDist '*') -Destination $package -Recurse
Move-Item (Join-Path $package 'electron.exe') (Join-Path $package 'OpenSchedule-App.exe')
foreach ($binary in @('openlab.exe', 'openschedule-agent.exe', 'openschedule.exe')) {
  Copy-Item (Join-Path $target "release\$binary") $package
}
Copy-Item (Join-Path $root 'README.md') $package
$app = Join-Path $package 'resources\app'
New-Item -ItemType Directory -Path (Join-Path $app 'electron'), (Join-Path $app 'node_modules\@phosphor-icons') -Force | Out-Null
Copy-Item (Join-Path $root 'package.json'), (Join-Path $root 'cli.js'), (Join-Path $root 'LICENSE') $app
Copy-Item (Join-Path $root 'electron\main.js'), (Join-Path $root 'electron\preload.js'), (Join-Path $root 'electron\gui-control.js') (Join-Path $app 'electron')
Copy-Item (Join-Path $root 'electron\renderer') (Join-Path $app 'electron') -Recurse
Copy-Item $icons (Join-Path $app 'node_modules\@phosphor-icons') -Recurse
Add-Type -AssemblyName System.IO.Compression.FileSystem
[System.IO.Compression.ZipFile]::CreateFromDirectory($package, $archive, [System.IO.Compression.CompressionLevel]::Optimal, $true)
Write-Output "배포 파일: $archive"
