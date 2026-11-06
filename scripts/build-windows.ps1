# Construit l'installateur Windows SasCompta (NSIS + MSI) sur une machine Windows.
# Prérequis: Node 20+, Rust stable, Visual Studio Build Tools (MSVC), WebView2 Runtime.
# Usage (PowerShell, depuis la racine du repo):
#   .\scripts\build-windows.ps1

$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot -Parent)

Write-Host "==> npm ci"
npm ci

Write-Host "==> rustup target add x86_64-pc-windows-msvc"
rustup target add x86_64-pc-windows-msvc

Write-Host "==> npm run build:windows"
npm run build:windows

$nsis = Get-ChildItem -Path "src-tauri\target\x86_64-pc-windows-msvc\release\bundle\nsis\*.exe" -ErrorAction SilentlyContinue
$msi  = Get-ChildItem -Path "src-tauri\target\x86_64-pc-windows-msvc\release\bundle\msi\*.msi" -ErrorAction SilentlyContinue

Write-Host ""
Write-Host "Artefacts:"
$nsis | ForEach-Object { Write-Host "  NSIS:" $_.FullName }
$msi  | ForEach-Object { Write-Host "  MSI: " $_.FullName }

if (-not $nsis -and -not $msi) {
  Write-Error "Aucun installateur trouvé — vérifier la sortie tauri build."
}
