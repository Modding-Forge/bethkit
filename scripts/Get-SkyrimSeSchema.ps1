[CmdletBinding()]
param(
    [System.IO.DirectoryInfo] $OutputDirectory = (
        Join-Path $PSScriptRoot '..\target\release-schemas'
    ),

    [System.IO.FileInfo] $MetadataFile = (
        Join-Path $PSScriptRoot '..\schemas\releases\skyrim_se.json'
    )
)

$ErrorActionPreference = 'Stop'
$metadata = Get-Content -LiteralPath $MetadataFile.FullName -Raw | ConvertFrom-Json
if ([int] $metadata.format_version -ne 1 -or [string] $metadata.game -ne 'skyrim_se') {
    throw 'Unsupported Skyrim Special Edition schema metadata'
}

$uri = [System.Uri] [string] $metadata.download_url
if ($uri.Scheme -ne 'https' -or $uri.Host -ne 'github.com') {
    throw "Schema download URL is not an approved GitHub URL: $uri"
}
if (-not $uri.AbsolutePath.StartsWith(
    '/Modding-Forge/xDump/releases/download/',
    [System.StringComparison]::OrdinalIgnoreCase
)) {
    throw "Schema download URL is outside Modding-Forge/xDump releases: $uri"
}

[System.IO.Directory]::CreateDirectory($OutputDirectory.FullName) | Out-Null
$destination = Join-Path $OutputDirectory.FullName 'skyrim_se.bkschema'
Invoke-WebRequest -Uri $uri -OutFile $destination -UseBasicParsing

$actualHash = (
    Get-FileHash -LiteralPath $destination -Algorithm SHA256
).Hash.ToLowerInvariant()
$expectedHash = ([string] $metadata.sha256).ToLowerInvariant()
if ($actualHash -ne $expectedHash) {
    Remove-Item -LiteralPath $destination -Force
    throw "Skyrim Special Edition schema SHA-256 mismatch: $actualHash"
}

Write-Output ([System.IO.Path]::GetFullPath($destination))
