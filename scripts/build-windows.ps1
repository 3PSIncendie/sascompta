# Construit l'installateur Windows SasCompta (NSIS + MSI) sur une machine Windows.
# Prérequis: Node 20+, Rust 1.90+, Visual Studio Build Tools (MSVC), WebView2 Runtime.
# Signature updater (obligatoire si createUpdaterArtifacts=true) :
#   $env:TAURI_SIGNING_PRIVATE_KEY = Get-Content $HOME\.tauri\sascompta.key -Raw
#   $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "…"   # optionnel
# Usage (PowerShell, depuis la racine du repo):
#   .\scripts\build-windows.ps1

$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot -Parent)

if (-not $env:TAURI_SIGNING_PRIVATE_KEY) {
  Write-Warning "TAURI_SIGNING_PRIVATE_KEY non défini — le build échouera tant que createUpdaterArtifacts est activé."
}

Write-Host "==> npm ci"
npm ci

Write-Host "==> rustup target add x86_64-pc-windows-msvc"
rustup target add x86_64-pc-windows-msvc

Write-Host "==> npm run build:windows"
npm run build:windows

$nsis = Get-ChildItem -Path "src-tauri\target\x86_64-pc-windows-msvc\release\bundle\nsis\*.exe" -ErrorAction SilentlyContinue
$msi  = Get-ChildItem -Path "src-tauri\target\x86_64-pc-windows-msvc\release\bundle\msi\*.msi" -ErrorAction SilentlyContinue
$sigs = Get-ChildItem -Path "src-tauri\target\x86_64-pc-windows-msvc\release\bundle\*\*.sig" -ErrorAction SilentlyContinue

Write-Host ""
Write-Host "Artefacts:"
$nsis | ForEach-Object { Write-Host "  NSIS:" $_.FullName }
$msi  | ForEach-Object { Write-Host "  MSI: " $_.FullName }
$sigs | ForEach-Object { Write-Host "  SIG: " $_.FullName }

if (-not $nsis -and -not $msi) {
  Write-Error "Aucun installateur trouvé — vérifier la sortie tauri build."
}
