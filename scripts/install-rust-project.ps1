[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$Rustup = (Get-Command rustup.exe -ErrorAction Stop).Source
$ProjectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$ToolchainFile = Get-Content -Raw (Join-Path $ProjectRoot "rust-toolchain.toml")
$Match = [regex]::Match($ToolchainFile, 'channel\s*=\s*"([^"]+)"')

if (-not $Match.Success) {
    throw "Unable to read the Rust channel from rust-toolchain.toml."
}

$Version = $Match.Groups[1].Value
$env:RUSTUP_HOME = Join-Path $ProjectRoot ".tools\rustup-windows"
$env:CARGO_HOME = Join-Path $ProjectRoot ".tools\cargo-windows"
New-Item -ItemType Directory -Force -Path $env:RUSTUP_HOME, $env:CARGO_HOME | Out-Null
$Rustc = Join-Path $env:RUSTUP_HOME "toolchains\$Version-x86_64-pc-windows-msvc\bin\rustc.exe"

if (Test-Path $Rustc) {
    & $Rustc --version
    exit $LASTEXITCODE
}

& $Rustup toolchain install $Version --profile minimal --target x86_64-pc-windows-msvc
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

if (-not (Test-Path $Rustc)) {
    throw "Rust installation completed without the expected rustc.exe."
}

& $Rustc --version
exit $LASTEXITCODE
