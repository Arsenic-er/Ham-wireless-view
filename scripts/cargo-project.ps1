# Ham Wireless View
# Project creator and lead developer: Arsenic-er
# SPDX-FileCopyrightText: 2026 Arsenic-er
# SPDX-License-Identifier: Apache-2.0

$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "enter-windows-build-environment.ps1")

Set-Location $ProjectRoot
& $Cargo @args
exit $LASTEXITCODE
