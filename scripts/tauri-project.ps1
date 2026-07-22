$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "enter-windows-build-environment.ps1")

Set-Location $ProjectRoot
& $Npm --prefix app run tauri -- @args
exit $LASTEXITCODE
