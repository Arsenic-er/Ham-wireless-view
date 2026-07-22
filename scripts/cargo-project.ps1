$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "enter-windows-build-environment.ps1")

Set-Location $ProjectRoot
& $Cargo @args
exit $LASTEXITCODE
