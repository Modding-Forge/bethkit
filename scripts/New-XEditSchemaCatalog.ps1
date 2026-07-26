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

    [System.IO.DirectoryInfo] $OutputDirectory = (
        Join-Path $PSScriptRoot '..\target\schemas'
    ),

    [switch] $AllowCandidate
)

$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
$rules = Join-Path $root 'xedit\conversion-rules.json'

$definitionsDirectory = Join-Path $OutputDirectory.FullName 'definitions'
$exportResult = & (Join-Path $PSScriptRoot 'Export-XEditDefinitions.ps1') `
    -Exporter $Exporter `
    -ExpectedExporterSha256 $ExpectedExporterSha256 `
    -ExpectedMapSha256 $ExpectedMapSha256 `
    -ExpectedPatchSha256 $ExpectedPatchSha256 `
    -ExpectedBuildSha256 $ExpectedBuildSha256 `
    -OutputDirectory $definitionsDirectory

$audit = Get-Content -LiteralPath $exportResult.callback_audit -Raw |
    ConvertFrom-Json
if ([int64] $audit.unclassified_bindings -ne 0) {
    throw (
        "Schema release is blocked by $($audit.unclassified_bindings) " +
        "unclassified callback/game bindings. Review " +
        $exportResult.callback_audit
    )
}

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
    $json = Join-Path $definitionsDirectory "$game.json"
    $package = Join-Path $OutputDirectory.FullName "$game.bkschema"
    $verification = Join-Path $OutputDirectory.FullName "$game.verify.bkschema"
    cargo run --locked -p bethkit-schema --bin bethkit-xedit-converter -- `
        $json $rules $package
    if ($LASTEXITCODE -ne 0) {
        throw "Schema conversion failed for $game"
    }
    cargo run --locked -p bethkit-schema --bin bethkit-xedit-converter -- `
        $json $rules $verification
    if ($LASTEXITCODE -ne 0) {
        throw "Schema verification conversion failed for $game"
    }
    $packageHash = (
        Get-FileHash -LiteralPath $package -Algorithm SHA256
    ).Hash
    $verificationHash = (
        Get-FileHash -LiteralPath $verification -Algorithm SHA256
    ).Hash
    if ($packageHash -ne $verificationHash) {
        throw "Schema package generation is not deterministic for $game"
    }
    $packages += $package
}

$compilerArguments = @(
    'run',
    '--locked',
    '-p',
    'bethkit-schema',
    '--bin',
    'bethkit-schema-compiler',
    '--',
    'verify-release'
) + $packages
if (-not $AllowCandidate) {
    cargo @compilerArguments
    if ($LASTEXITCODE -ne 0) {
        throw 'Schema packages did not pass the official release gates'
    }
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
