[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [System.IO.FileInfo] $Exporter,

    [Parameter(Mandatory)]
    [ValidatePattern('^[0-9a-fA-F]{64}$')]
    [string] $ExpectedExporterSha256,

    [Parameter(Mandatory)]
    [ValidatePattern('^[0-9a-fA-F]{64}$')]
    [string] $ExpectedPatchSha256,

    [Parameter(Mandatory)]
    [ValidatePattern('^[0-9a-fA-F]{64}$')]
    [string] $ExpectedBuildSha256,

    [System.IO.DirectoryInfo] $OutputDirectory = (
        Join-Path $PSScriptRoot '..\target\schemas'
    )
)

$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
$rules = Join-Path $root 'xedit\conversion-rules.json'

& (Join-Path $PSScriptRoot 'Test-XEditExporter.ps1') `
    -Exporter $Exporter `
    -ExpectedExporterSha256 $ExpectedExporterSha256 `
    -ExpectedPatchSha256 $ExpectedPatchSha256 `
    -ExpectedBuildSha256 $ExpectedBuildSha256 | Out-Null

New-Item -ItemType Directory -Force -Path $OutputDirectory.FullName | Out-Null
$games = @(
    'skyrim_le',
    'skyrim_se',
    'skyrim_vr',
    'fallout_3',
    'fallout_nv',
    'fallout_4',
    'fallout_4_vr',
    'fallout_76',
    'oblivion',
    'morrowind',
    'starfield'
)

$packages = @()
foreach ($game in $games) {
    $json = Join-Path $OutputDirectory.FullName "$game.json"
    $package = Join-Path $OutputDirectory.FullName "$game.bkschema"
    & $Exporter.FullName --bethkit-export --game $game --output $json
    if ($LASTEXITCODE -ne 0) {
        throw "Exporter failed for $game with exit code $LASTEXITCODE"
    }
    cargo run --locked -p bethkit-schema --bin bethkit-xedit-converter -- `
        $json $rules $package
    if ($LASTEXITCODE -ne 0) {
        throw "Schema conversion failed for $game"
    }
    $packages += $package
}

$bundleA = Join-Path $OutputDirectory.FullName 'bethkit.bkschemas'
$bundleB = Join-Path $OutputDirectory.FullName 'bethkit.verify.bkschemas'
cargo run --locked -p bethkit-schema --bin bethkit-schema-compiler -- `
    bundle @packages $bundleA
cargo run --locked -p bethkit-schema --bin bethkit-schema-compiler -- `
    bundle @packages $bundleB

$hashA = (Get-FileHash -LiteralPath $bundleA -Algorithm SHA256).Hash
$hashB = (Get-FileHash -LiteralPath $bundleB -Algorithm SHA256).Hash
if ($hashA -ne $hashB) {
    throw 'Schema catalog generation is not deterministic'
}

Write-Output $bundleA
