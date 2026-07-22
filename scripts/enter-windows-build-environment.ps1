$ErrorActionPreference = "Stop"
$ProjectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$ToolchainFile = Get-Content -Raw (Join-Path $ProjectRoot "rust-toolchain.toml")
$Match = [regex]::Match($ToolchainFile, 'channel\s*=\s*"([^"]+)"')

if (-not $Match.Success) {
    throw "Unable to read the Rust channel from rust-toolchain.toml."
}

$RustVersion = $Match.Groups[1].Value
$env:RUSTUP_HOME = Join-Path $ProjectRoot ".tools\rustup-windows"
$env:CARGO_HOME = Join-Path $ProjectRoot ".tools\cargo-windows"
$env:CARGO_TARGET_DIR = Join-Path $ProjectRoot ".tools\target-windows"
$ToolchainBin = Join-Path $env:RUSTUP_HOME "toolchains\$RustVersion-x86_64-pc-windows-msvc\bin"
$Cargo = Join-Path $ToolchainBin "cargo.exe"
$Rustc = Join-Path $ToolchainBin "rustc.exe"

if (-not (Test-Path $Cargo) -or -not (Test-Path $Rustc)) {
    throw "Project-local Rust is missing. Run scripts\install-rust-project.ps1 first."
}

$NodeVersion = (Get-Content -Raw (Join-Path $ProjectRoot ".node-version")).Trim()
$NodeDirectory = Join-Path $ProjectRoot ".tools\node-v$NodeVersion-win-x64"
$Npm = Join-Path $NodeDirectory "npm.cmd"
if (-not (Test-Path $Npm)) {
    throw "Project-local Node.js is missing. Run scripts\install-node-project.ps1 first."
}

$env:PATH = "$NodeDirectory;$ToolchainBin;$env:PATH"
$env:RUSTC = $Rustc
$env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = "-C target-feature=+crt-static"
$env:NPM_CONFIG_CACHE = Join-Path $ProjectRoot ".tools\npm-cache-windows"

$VsWhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path $VsWhere)) {
    throw "Visual Studio Installer's vswhere.exe was not found."
}

$InstallationPath = (& $VsWhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath | Select-Object -First 1).Trim()
if (-not $InstallationPath) {
    throw "Visual Studio C++ Build Tools were not found."
}

$VcVars = Join-Path $InstallationPath "VC\Auxiliary\Build\vcvars64.bat"
if (-not (Test-Path $VcVars)) {
    throw "vcvars64.bat was not found at $VcVars."
}

$DeveloperEnvironment = & $env:ComSpec /d /s /c "call `"$VcVars`" >nul && set"
if ($LASTEXITCODE -ne 0) {
    throw "Unable to initialize the Visual Studio x64 developer environment."
}

foreach ($Line in $DeveloperEnvironment) {
    $Separator = $Line.IndexOf('=')
    if ($Separator -gt 0) {
        $Name = $Line.Substring(0, $Separator)
        $Value = $Line.Substring($Separator + 1)
        Set-Item -Path "Env:$Name" -Value $Value
    }
}

# vcvars64 may rewrite PATH while importing the compiler environment. Keep the
# project-pinned runtimes first so npm scripts and Cargo subprocesses resolve
# the same Node.js and Rust versions as their wrappers.
$env:PATH = "$NodeDirectory;$ToolchainBin;$env:PATH"
