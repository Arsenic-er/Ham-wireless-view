$ErrorActionPreference = "Stop"
$ProjectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$Version = (Get-Content -Raw (Join-Path $ProjectRoot ".node-version")).Trim()
$NodeDirectory = Join-Path $ProjectRoot ".tools\node-v$Version-win-x64"
$Npm = Join-Path $NodeDirectory "npm.cmd"

if (-not (Test-Path $Npm)) {
    throw "Project-local Node.js is missing. Run scripts\install-node-project.ps1 first."
}

$env:PATH = "$NodeDirectory;$env:PATH"
$env:NPM_CONFIG_CACHE = Join-Path $ProjectRoot ".tools\npm-cache-windows"
Set-Location $ProjectRoot

& $Npm @args
exit $LASTEXITCODE
