[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [System.IO.FileInfo] $Exporter,

    [Parameter(Mandatory)]
    [ValidatePattern('^[0-9a-fA-F]{64}$')]
    [string] $ExpectedExporterSha256,

    [Parameter(Mandatory)]
    [ValidatePattern('^[0-9a-fA-F]{64}$')]
    [string] $ExpectedMapSha256,

    [Parameter(Mandatory)]
    [ValidatePattern('^[0-9a-fA-F]{64}$')]
    [string] $ExpectedPatchSha256,

    [Parameter(Mandatory)]
    [ValidatePattern('^[0-9a-fA-F]{64}$')]
    [string] $ExpectedBuildSha256,

    [System.IO.FileInfo] $LockFile = (
        Join-Path $PSScriptRoot '..\xedit-source.lock'
    )
)

$ErrorActionPreference = 'Stop'
$lock = Get-Content -LiteralPath $LockFile.FullName -Raw | ConvertFrom-Json
$actualBinaryHash = (
    Get-FileHash -LiteralPath $Exporter.FullName -Algorithm SHA256
).Hash.ToLowerInvariant()
$mapPath = [System.IO.Path]::ChangeExtension($Exporter.FullName, '.map')
if (-not (Test-Path -LiteralPath $mapPath -PathType Leaf)) {
    throw "Exporter MAP file not found: $mapPath"
}
$actualMapHash = (
    Get-FileHash -LiteralPath $mapPath -Algorithm SHA256
).Hash.ToLowerInvariant()

if ($actualBinaryHash -ne $ExpectedExporterSha256.ToLowerInvariant()) {
    throw "Exporter SHA-256 mismatch: $actualBinaryHash"
}
if ($actualMapHash -ne $ExpectedMapSha256.ToLowerInvariant()) {
    throw "Exporter MAP SHA-256 mismatch: $actualMapHash"
}

$provenanceText = & $Exporter.FullName --bethkit-provenance
if ($LASTEXITCODE -ne 0) {
    throw "Exporter provenance command failed with exit code $LASTEXITCODE"
}
$provenance = $provenanceText | ConvertFrom-Json

$expected = @{
    source_tag = $lock.tag
    source_commit = $lock.commit
    source_archive_sha256 = $lock.archive_sha256
    exporter_binary_sha256 = $ExpectedExporterSha256
    exporter_map_sha256 = $ExpectedMapSha256
    exporter_patch_sha256 = $ExpectedPatchSha256
    exporter_build_sha256 = $ExpectedBuildSha256
}

foreach ($entry in $expected.GetEnumerator()) {
    $actual = [string] $provenance.($entry.Key)
    if ($actual.ToLowerInvariant() -ne $entry.Value.ToLowerInvariant()) {
        throw "Provenance mismatch for $($entry.Key): $actual"
    }
}

$provenance
