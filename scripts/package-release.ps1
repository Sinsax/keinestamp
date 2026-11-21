<#
.SYNOPSIS
  Build uniformly named release artifacts (installers / portable / CLI / SHA-256).

.DESCRIPTION
  Naming: Keinestamp_<version>_<os>-<arch>_<kind>   (the OS must be spelled out)
    Windows:
      Keinestamp_<ver>_windows-x64_setup.exe      NSIS installer (recommended)
      Keinestamp_<ver>_windows-x64_en-US.msi      MSI installer
      Keinestamp_<ver>_windows-x64_portable.zip   portable GUI + lore-bin + docs
      Keinestamp_<ver>_windows-x64_cli.zip        CLI + lore-bin + docs
    Linux (typical server target; CLI only, no GUI installer):
      Keinestamp_<ver>_linux-x64_cli.tar.gz       CLI + lore-bin + docs + serve.toml sample
    SHA256SUMS.txt                                `sha256sum -c` compatible

  Prerequisites: run `cargo build --release` and (Windows only) `cargo tauri build` first.

.EXAMPLE
  powershell -File scripts/package-release.ps1
  powershell -File scripts/package-release.ps1 -Version 0.5.3 -Platform linux-x64
#>
param(
    [string]$Version = '',
    [string]$Platform = '',
    [string]$OutDir = 'dist/release'
)

$ErrorActionPreference = 'Stop'

# $PSScriptRoot may be empty depending on how the script is invoked; fall back step by step.
$scriptDir = $PSScriptRoot
if (-not $scriptDir) { $scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path }
if (-not $scriptDir) { $scriptDir = (Get-Location).Path }
$root = Split-Path -Parent $scriptDir
if (-not (Test-Path (Join-Path $root 'Cargo.toml'))) { throw "Cannot locate repo root from $scriptDir" }

if (-not $Version) {
    $manifest = Get-Content (Join-Path $root 'Cargo.toml') -Raw
    if ($manifest -match '(?m)^version\s*=\s*"([^"]+)"') {
        $Version = $Matches[1]
    } else {
        throw 'Cannot read version from Cargo.toml'
    }
}

if (-not $Platform) {
    $arch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'arm64' } else { 'x64' }
    if ($IsWindows -or $env:OS -eq 'Windows_NT') { $Platform = "windows-$arch" }
    elseif ($IsMacOS) { $Platform = "macos-$arch" }
    else { $Platform = "linux-$arch" }
}

$out = Join-Path $root $OutDir
New-Item -ItemType Directory -Force -Path $out | Out-Null
$prefix = "Keinestamp_{0}_{1}" -f $Version, $Platform
$isWindowsPlatform = $Platform -like 'windows-*'

$stage = Join-Path ([System.IO.Path]::GetTempPath()) ("keinestamp_pkg_" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $stage | Out-Null

# lore-bin is needed by both platforms (CLI + portable); resolve it before the helper.
$loreBin = Join-Path $root 'target\release\lore-bin'
if (-not (Test-Path $loreBin)) { throw "Missing build artifact: $loreBin" }

function Copy-Common($dest) {
    Copy-Item $loreBin (Join-Path $dest 'lore-bin') -Recurse -Force
    foreach ($doc in @('README.md', 'README.zh-CN.md')) {
        $path = Join-Path $root $doc
        if (Test-Path $path) { Copy-Item $path (Join-Path $dest $doc) -Force }
    }
    $serveSample = Join-Path $root 'docs\serve.toml.sample'
    if (Test-Path $serveSample) { Copy-Item $serveSample (Join-Path $dest 'serve.toml') -Force }
}

try {
    if ($isWindowsPlatform) {
        $guiExe = Join-Path $root 'src-tauri\target\release\keinestamp-tauri.exe'
        $nsis = Join-Path $root ("src-tauri\target\release\bundle\nsis\Keinestamp_{0}_x64-setup.exe" -f $Version)
        $msi = Join-Path $root ("src-tauri\target\release\bundle\msi\Keinestamp_{0}_x64_en-US.msi" -f $Version)
        foreach ($p in @($guiExe, $nsis, $msi)) {
            if (-not (Test-Path $p)) { throw "Missing build artifact: $p" }
        }

        # 1) installers: copy and normalize names
        Copy-Item $nsis (Join-Path $out ("{0}-setup.exe" -f $prefix)) -Force
        Copy-Item $msi (Join-Path $out ("{0}_en-US.msi" -f $prefix)) -Force

        # 2) portable: GUI exe renamed to Keinestamp.exe + lore-bin + docs
        $portable = Join-Path $stage 'portable'
        New-Item -ItemType Directory -Force -Path $portable | Out-Null
        Copy-Item $guiExe (Join-Path $portable 'Keinestamp.exe') -Force
        Copy-Common $portable
        Compress-Archive -Path (Join-Path $portable '*') -DestinationPath (Join-Path $out ("{0}_portable.zip" -f $prefix)) -Force
    }

    # 3) CLI package (zip on Windows, tar.gz on Linux)
    $cliExe = Join-Path $root 'target\release\keinestamp.exe'
    if (-not $isWindowsPlatform) { $cliExe = Join-Path $root 'target/release/keinestamp' }
    if (-not (Test-Path $cliExe)) { throw "Missing build artifact: $cliExe" }

    $cli = Join-Path $stage 'cli'
    New-Item -ItemType Directory -Force -Path $cli | Out-Null
    $cliName = 'keinestamp.exe'
    if (-not $isWindowsPlatform) { $cliName = 'keinestamp' }
    Copy-Item $cliExe (Join-Path $cli $cliName) -Force
    Copy-Common $cli

    if ($isWindowsPlatform) {
        Compress-Archive -Path (Join-Path $cli '*') -DestinationPath (Join-Path $out ("{0}_cli.zip" -f $prefix)) -Force
    } else {
        $tgz = Join-Path $out ("{0}_cli.tar.gz" -f $prefix)
        & tar -czf $tgz -C $cli .
        if ($LASTEXITCODE -ne 0) { throw 'tar failed' }
    }

    # 4) SHA-256 sums (no BOM, `sha256sum -c` compatible)
    $files = Get-ChildItem $out -File | Where-Object { $_.Name -ne 'SHA256SUMS.txt' } | Sort-Object Name
    $lines = $files | ForEach-Object {
        '{0}  {1}' -f (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower(), $_.Name
    }
    $sumPath = Join-Path $out 'SHA256SUMS.txt'
    [System.IO.File]::WriteAllLines($sumPath, $lines, (New-Object System.Text.UTF8Encoding($false)))

    Write-Host ''
    Write-Host ("Release artifacts ({0} / {1}) -> {2}" -f $Version, $Platform, $out) -ForegroundColor Green
    foreach ($f in (Get-ChildItem $out -File | Sort-Object Name)) {
        Write-Host ("  {0,-48} {1,10:N1} MB" -f $f.Name, ($f.Length / 1MB))
    }
}
finally {
    if (Test-Path $stage) { Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue }
}
