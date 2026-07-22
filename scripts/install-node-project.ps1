[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$ProjectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$Version = (Get-Content -Raw (Join-Path $ProjectRoot ".node-version")).Trim()
$ArchiveName = "node-v$Version-win-x64.zip"
$NodeDirectory = Join-Path $ProjectRoot ".tools\node-v$Version-win-x64"
$DownloadDirectory = Join-Path $ProjectRoot ".tools\downloads"
$ArchivePath = Join-Path $DownloadDirectory $ArchiveName

$ExpectedHashes = @{
    "24.18.0" = "0ae68406b42d7725661da979b1403ec9926da205c6770827f33aac9d8f26e821"
}

if (-not $ExpectedHashes.ContainsKey($Version)) {
    throw "No trusted Node.js archive hash is recorded for version $Version."
}

if (Test-Path (Join-Path $NodeDirectory "node.exe")) {
    & (Join-Path $NodeDirectory "node.exe") --version
    exit $LASTEXITCODE
}

New-Item -ItemType Directory -Force -Path $DownloadDirectory | Out-Null
$Url = "https://nodejs.org/dist/v$Version/$ArchiveName"

if (-not (Test-Path $ArchivePath)) {
    Write-Host "Downloading Node.js $Version from $Url"
    Invoke-WebRequest -Uri $Url -OutFile $ArchivePath -UseBasicParsing
}

$ActualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $ArchivePath).Hash.ToLowerInvariant()
$ExpectedHash = $ExpectedHashes[$Version].ToLowerInvariant()
if ($ActualHash -ne $ExpectedHash) {
    throw "Node.js archive checksum mismatch. Expected $ExpectedHash, received $ActualHash."
}

Expand-Archive -LiteralPath $ArchivePath -DestinationPath (Join-Path $ProjectRoot ".tools") -Force
if (-not (Test-Path (Join-Path $NodeDirectory "node.exe"))) {
    throw "Node.js extraction completed without the expected node.exe."
}

& (Join-Path $NodeDirectory "node.exe") --version
exit $LASTEXITCODE
