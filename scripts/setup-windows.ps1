# One-time developer setup for Windows 10/11. Run in PowerShell:
#   powershell -ExecutionPolicy Bypass -File scripts\setup-windows.ps1
$ErrorActionPreference = 'Stop'

function Have($cmd) { [bool](Get-Command $cmd -ErrorAction SilentlyContinue) }

Write-Host '==> MSVC build tools (C++ workload)'
winget install --id Microsoft.VisualStudio.2022.BuildTools --silent --accept-package-agreements --accept-source-agreements `
  --override '--wait --quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended'

if (-not (Have 'make')) {
  Write-Host '==> GNU make (for the Makefile tasks)'
  winget install --id ezwinports.make --silent --accept-package-agreements --accept-source-agreements
}

if (-not (Have 'rustup')) {
  Write-Host '==> Installing rustup'
  winget install --id Rustlang.Rustup --silent --accept-package-agreements --accept-source-agreements
  $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
}

Set-Location (Join-Path $PSScriptRoot '..')
Write-Host '==> Installing toolchain from rust-toolchain.toml'
rustup toolchain install

if (-not (Have 'cargo-binstall')) {
  Write-Host '==> Installing cargo-binstall'
  Set-ExecutionPolicy Unrestricted -Scope Process
  Invoke-Expression (Invoke-WebRequest 'https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.ps1').Content
}

Write-Host '==> Installing cargo tools'
cargo binstall -y cargo-deny cargo-nextest cargo-packager trunk

Write-Host '==> Done. Open a new terminal, then: make run'
