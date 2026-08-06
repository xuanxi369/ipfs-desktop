param(
    [string]$Version = "v0.42.0"
)

$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $PSScriptRoot
$resourceDir = Join-Path $projectRoot "src-tauri\resources"
$archiveName = "kubo_${Version}_windows-amd64.zip"
$baseUrl = "https://dist.ipfs.tech/kubo/$Version"
$workDir = Join-Path ([System.IO.Path]::GetTempPath()) ("ipfs-desktop-kubo-" + [guid]::NewGuid().ToString("N"))

try {
    New-Item -ItemType Directory -Force -Path $resourceDir | Out-Null
    New-Item -ItemType Directory -Force -Path $workDir | Out-Null

    $archivePath = Join-Path $workDir $archiveName
    $checksumPath = Join-Path $workDir "${archiveName}.sha512"
    Write-Host "Downloading official Kubo $Version..."
    Invoke-WebRequest -Uri "$baseUrl/$archiveName" -OutFile $archivePath
    Invoke-WebRequest -Uri "$baseUrl/${archiveName}.sha512" -OutFile $checksumPath

    $expected = ((Get-Content -LiteralPath $checksumPath -Raw).Trim() -split '\s+')[0].ToLowerInvariant()
    $actual = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA512).Hash.ToLowerInvariant()
    if ($expected -ne $actual) {
        throw "Kubo archive checksum mismatch. Expected $expected, got $actual"
    }

    Expand-Archive -LiteralPath $archivePath -DestinationPath $workDir -Force
    $source = Get-ChildItem -LiteralPath $workDir -Recurse -Filter "ipfs.exe" -File | Select-Object -First 1
    if (-not $source) {
        throw "The downloaded Kubo archive does not contain ipfs.exe"
    }

    Copy-Item -LiteralPath $source.FullName -Destination (Join-Path $resourceDir "ipfs.exe") -Force
    & (Join-Path $resourceDir "ipfs.exe") version
    Write-Host "Kubo is ready at src-tauri/resources/ipfs.exe"
}
finally {
    if (Test-Path -LiteralPath $workDir) {
        Remove-Item -LiteralPath $workDir -Recurse -Force
    }
}
